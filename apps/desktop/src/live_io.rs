//! Live sessions on the desktop: `gigcraft-live-net` behind the UI's `LiveIo`.

use gigcraft_live_net::{ClientEvent, LiveClient, LiveServer, NetEvent};
use gigcraft_ui_egui::{HostEvent, Hosted, JoinEvent, LiveIo};

#[derive(Default)]
pub struct Net {
    server: Option<LiveServer>,
    client: Option<LiveClient>,
}

impl LiveIo for Net {
    fn host(&mut self) -> Result<Hosted, String> {
        let server =
            LiveServer::start(gigcraft_live_net::DEFAULT_PORT).map_err(|e| e.to_string())?;
        let urls = server.urls();
        self.server = Some(server);
        let session = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| format!("{:x}", d.as_nanos()))
            .unwrap_or_else(|_| "session".into());
        Ok(Hosted { urls, session })
    }

    fn stop_host(&mut self) {
        if let Some(s) = self.server.take() {
            s.stop();
        }
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
