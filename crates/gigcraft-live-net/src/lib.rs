//! Live sessions on the local network.
//!
//! The leading device runs a [`LiveServer`]: one TCP port that serves the
//! band's web viewer (`/`) and the WebSocket (`/ws`) that carries the
//! `gigcraft-live` messages. Band members open the address on a phone or
//! tablet (QR code from [`qr`]), or another GigCraft joins with a
//! [`LiveClient`].
//!
//! Threads do the blocking network work; the app polls events once per frame
//! and sends text back. Nothing leaves the local network: there is no relay,
//! no account, and no TLS (browsers on the LAN cannot verify a certificate for
//! a private address anyway). Anyone on the network can join as a follower;
//! followers can only say "ready" and send cues.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use tungstenite::protocol::WebSocketConfig;
use tungstenite::{Message, WebSocket};

pub use gigcraft_live::{DEFAULT_PORT, ws_url};
/// Largest WebSocket message accepted (a snapshot carries a chart).
pub const MAX_MESSAGE_BYTES: usize = 4 << 20;
/// The band's web viewer, served at `/`.
pub const VIEWER_HTML: &str = include_str!("../web/index.html");

const POLL: Duration = Duration::from_millis(40);

/// A connection id, unique per server.
pub type ConnId = u64;

/// Something that happened on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetEvent {
    Connected { conn: ConnId, peer: String },
    Text { conn: ConnId, text: String },
    Closed { conn: ConnId },
}

enum Outgoing {
    Text(String),
    Close,
}

type Peers = Arc<Mutex<HashMap<ConnId, Sender<Outgoing>>>>;

fn ws_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

/// The leader's server: web viewer plus WebSocket on one port.
pub struct LiveServer {
    port: u16,
    events: Receiver<NetEvent>,
    peers: Peers,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for LiveServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveServer")
            .field("port", &self.port)
            .finish()
    }
}

impl LiveServer {
    /// Listen on all interfaces, on `port` or, if that is taken, on any free port.
    pub fn start(port: u16) -> io::Result<Self> {
        let listener =
            TcpListener::bind(("0.0.0.0", port)).or_else(|_| TcpListener::bind(("0.0.0.0", 0)))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let (tx, events) = mpsc::channel();
        let peers: Peers = Arc::default();
        let stop = Arc::new(AtomicBool::new(false));
        let next = Arc::new(AtomicU64::new(1));
        {
            let peers = peers.clone();
            let stop = stop.clone();
            thread::Builder::new()
                .name("gigcraft-live-accept".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((stream, addr)) => {
                                let conn = next.fetch_add(1, Ordering::Relaxed);
                                let (peers, stop, tx) = (peers.clone(), stop.clone(), tx.clone());
                                let _ = thread::Builder::new()
                                    .name(format!("gigcraft-live-{conn}"))
                                    .spawn(move || serve(stream, addr, conn, &peers, &stop, &tx));
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                thread::sleep(POLL);
                            }
                            Err(_) => thread::sleep(POLL),
                        }
                    }
                })?;
        }
        Ok(Self {
            port,
            events,
            peers,
            stop,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Addresses band members can open (`http://192.168.1.20:8787/`).
    pub fn urls(&self) -> Vec<String> {
        lan_urls(self.port)
    }

    /// Events since the last call.
    pub fn poll(&self) -> Vec<NetEvent> {
        self.events.try_iter().collect()
    }

    fn peer(&self, conn: ConnId) -> Option<Sender<Outgoing>> {
        self.peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&conn)
            .cloned()
    }

    /// Send text to one connection (dropped if it is gone).
    pub fn send(&self, conn: ConnId, text: String) {
        if let Some(p) = self.peer(conn) {
            let _ = p.send(Outgoing::Text(text));
        }
    }

    /// Close one connection after what was sent before.
    pub fn close(&self, conn: ConnId) {
        if let Some(p) = self.peer(conn) {
            let _ = p.send(Outgoing::Close);
        }
    }

    /// Number of open WebSocket connections.
    pub fn connections(&self) -> usize {
        self.peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Stop listening and close every connection.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for LiveServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One accepted TCP connection: a page request or a WebSocket.
fn serve(
    stream: TcpStream,
    addr: SocketAddr,
    conn: ConnId,
    peers: &Peers,
    stop: &AtomicBool,
    events: &Sender<NetEvent>,
) {
    if stream.set_nonblocking(false).is_err() || stream.set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let Some(head) = peek_request_line(&stream) else {
        return;
    };
    if head.starts_with("GET /ws") {
        let Ok(mut ws) = tungstenite::accept_with_config(stream, Some(ws_config())) else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(conn, tx);
        let _ = events.send(NetEvent::Connected {
            conn,
            peer: addr.to_string(),
        });
        pump(&mut ws, &rx, stop, |text| {
            let _ = events.send(NetEvent::Text { conn, text });
        });
        peers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&conn);
        let _ = events.send(NetEvent::Closed { conn });
    } else {
        page(stream, &head);
    }
}

/// The request line, without consuming it (the WebSocket handshake reads it again).
fn peek_request_line(stream: &TcpStream) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut buf = [0u8; 512];
    while Instant::now() < deadline {
        match stream.peek(&mut buf) {
            Ok(0) => return None,
            Ok(n) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                if let Some(line) = text.split("\r\n").next().filter(|_| text.contains("\r\n")) {
                    return Some(line.to_string());
                }
                if n == buf.len() {
                    return None;
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Answer a plain HTTP request: the viewer, or 404.
fn page(mut stream: TcpStream, request_line: &str) {
    // Read (and discard) the request head, bounded.
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    let deadline = Instant::now() + Duration::from_secs(2);
    while head.len() < 16 * 1024 && !head.ends_with(b"\r\n\r\n") && Instant::now() < deadline {
        match stream.read(&mut byte) {
            Ok(1) => head.push(byte[0]),
            Ok(_) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => break,
        }
    }
    let path = request_line.split(' ').nth(1).unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/");
    let (status, kind, body) = match (request_line.starts_with("GET "), path) {
        (true, "/" | "/index.html") => ("200 OK", "text/html; charset=utf-8", VIEWER_HTML),
        (true, _) => ("404 Not Found", "text/plain; charset=utf-8", "not found"),
        (false, _) => (
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            "GET only",
        ),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

/// Move messages both ways until the socket closes or `stop` is set.
fn pump<S: Read + Write>(
    ws: &mut WebSocket<S>,
    outgoing: &Receiver<Outgoing>,
    stop: &AtomicBool,
    mut on_text: impl FnMut(String),
) {
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = ws.close(None);
            let _ = ws.flush();
            return;
        }
        for out in outgoing.try_iter() {
            match out {
                Outgoing::Text(t) => {
                    if ws.send(Message::text(t)).is_err() {
                        return;
                    }
                }
                Outgoing::Close => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return;
                }
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => on_text(t.to_string()),
            Ok(Message::Close(_)) => {
                let _ = ws.flush();
                return;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
        // Pongs and queued frames.
        match ws.flush() {
            Ok(()) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
    }
}

/// What a follower's connection reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    Connected,
    Text(String),
    /// The connection ended (or never started), with the reason.
    Closed(String),
}

/// A follower's connection to a leader.
pub struct LiveClient {
    url: String,
    events: Receiver<ClientEvent>,
    out: Sender<Outgoing>,
    stop: Arc<AtomicBool>,
}

impl std::fmt::Debug for LiveClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveClient")
            .field("url", &self.url)
            .finish()
    }
}

impl LiveClient {
    /// Connect in the background; events tell how it went.
    pub fn connect(address: &str) -> Self {
        let (tx, events) = mpsc::channel();
        let (out, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let url = ws_url(address).unwrap_or_default();
        let (thread_url, thread_stop, thread_tx) = (url.clone(), stop.clone(), tx.clone());
        let spawned = thread::Builder::new()
            .name("gigcraft-live-client".into())
            .spawn(move || {
                let reason = match open(&thread_url) {
                    Ok(mut ws) => {
                        let _ = thread_tx.send(ClientEvent::Connected);
                        pump(&mut ws, &rx, &thread_stop, |text| {
                            let _ = thread_tx.send(ClientEvent::Text(text));
                        });
                        "connection closed".to_string()
                    }
                    Err(e) => e,
                };
                let _ = thread_tx.send(ClientEvent::Closed(reason));
            });
        if let Err(e) = spawned {
            let _ = tx.send(ClientEvent::Closed(e.to_string()));
        }
        Self {
            url,
            events,
            out,
            stop,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn poll(&self) -> Vec<ClientEvent> {
        self.events.try_iter().collect()
    }

    pub fn send(&self, text: String) {
        let _ = self.out.send(Outgoing::Text(text));
    }

    pub fn close(&self) {
        let _ = self.out.send(Outgoing::Close);
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for LiveClient {
    fn drop(&mut self) {
        self.close();
    }
}

fn open(url: &str) -> Result<WebSocket<TcpStream>, String> {
    if url.is_empty() {
        return Err("not an address (expected e.g. 192.168.1.20:8787)".into());
    }
    let host = url
        .strip_prefix("ws://")
        .and_then(|r| r.split('/').next())
        .ok_or("bad address")?;
    let addr = host
        .to_socket_addrs()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("address not found")?;
    let stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let (ws, _) = tungstenite::client::client_with_config(url, stream, Some(ws_config()))
        .map_err(|e| e.to_string())?;
    ws.get_ref()
        .set_read_timeout(Some(POLL))
        .map_err(|e| e.to_string())?;
    Ok(ws)
}

/// `http://<address>:<port>/` for each IPv4 address of this computer on a
/// network, the likely Wi-Fi/Ethernet address first (home and venue routers
/// use 192.168.x.x; VPN and virtual interfaces last). No loopback.
pub fn lan_urls(port: u16) -> Vec<String> {
    const VIRTUAL: [&str; 10] = [
        "utun", "tun", "tap", "wg", "ppp", "ipsec", "bridge", "vmnet", "docker", "veth",
    ];
    let mut ips: Vec<(u8, std::net::Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(v4) if !v4.is_link_local() => {
                let rank = if VIRTUAL.iter().any(|v| i.name.starts_with(v)) {
                    3
                } else if v4.octets()[..2] == [192, 168] {
                    0
                } else if v4.is_private() {
                    1
                } else {
                    2
                };
                Some((rank, v4))
            }
            _ => None,
        })
        .collect();
    ips.sort();
    ips.dedup_by_key(|(_, ip)| *ip);
    ips.into_iter()
        .map(|(_, ip)| format!("http://{ip}:{port}/"))
        .collect()
}

/// A QR code for `text` as rows of dark (true) / light modules, or `None` if
/// the text is too long for a QR code.
pub fn qr(text: &str) -> Option<Vec<Vec<bool>>> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    Some(
        colors
            .chunks(width)
            .map(|row| row.iter().map(|c| *c == qrcode::Color::Dark).collect())
            .collect(),
    )
}

#[cfg(test)]
mod tests;
