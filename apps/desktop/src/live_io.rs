//! Live sessions on the desktop: `gigcraft-live-net` behind the UI's `LiveIo`.

use gigcraft_live_net::{ClientEvent, Discovery, LiveClient, LiveServer, NetEvent};
use gigcraft_ui_egui::{HostEvent, Hosted, JoinEvent, LiveIo};

#[derive(Default)]
pub struct Net {
    server: Option<LiveServer>,
    client: Option<LiveClient>,
    discovery: Option<Discovery>,
}

impl LiveIo for Net {
    fn host(&mut self, name: &str) -> Result<Hosted, String> {
        let mut server =
            LiveServer::start(gigcraft_live_net::DEFAULT_PORT).map_err(|e| e.to_string())?;
        // Followers can still join by address if the announcement fails.
        let _ = server.announce(name);
        let urls = server.urls();
        self.server = Some(server);
        let session = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| format!("{:x}", d.as_nanos()))
            .unwrap_or_else(|_| "session".into());
        Ok(Hosted { urls, session })
    }

    fn stop_host(&mut self) {
        if let Some(mut s) = self.server.take() {
            s.stop();
        }
    }

    fn browse(&mut self) -> Vec<(String, String)> {
        if self.discovery.is_none() {
            self.discovery = Discovery::start().ok();
        }
        self.discovery
            .as_mut()
            .map(|d| {
                d.leaders()
                    .into_iter()
                    .map(|l| (l.name, l.address))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn poll_host(&mut self) -> Vec<HostEvent> {
        let Some(s) = &self.server else {
            return Vec::new();
        };
        s.poll()
            .into_iter()
            .map(|e| match e {
                NetEvent::Connected { conn, peer } => HostEvent::Connected { conn, peer },
                NetEvent::Text { conn, text } => HostEvent::Text { conn, text },
                NetEvent::Closed { conn } => HostEvent::Closed { conn },
            })
            .collect()
    }

    fn send(&mut self, conn: u64, text: String) {
        if let Some(s) = &self.server {
            s.send(conn, text);
        }
    }

    fn close(&mut self, conn: u64) {
        if let Some(s) = &self.server {
            s.close(conn);
        }
    }

    fn join(&mut self, address: &str) {
        self.client = Some(LiveClient::connect(address));
    }

    fn leave(&mut self) {
        if let Some(c) = self.client.take() {
            c.close();
        }
    }

    fn poll_join(&mut self) -> Vec<JoinEvent> {
        let Some(c) = &self.client else {
            return Vec::new();
        };
        c.poll()
            .into_iter()
            .map(|e| match e {
                ClientEvent::Connected => JoinEvent::Connected,
                ClientEvent::Text(t) => JoinEvent::Text(t),
                ClientEvent::Closed(r) => JoinEvent::Closed(r),
            })
            .collect()
    }

    fn send_to_leader(&mut self, text: String) {
        if let Some(c) = &self.client {
            c.send(text);
        }
    }

    fn qr(&self, text: &str) -> Option<Vec<Vec<bool>>> {
        gigcraft_live_net::qr(text)
    }
}
