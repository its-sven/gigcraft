//! Leading a live session from iOS (ADR-0013): the same Rust server and
//! leader state as on the desktop. Swift only announces the session with
//! Bonjour (the system's mDNS service) and calls `pump` a few times a second.

use crate::{Core, CoreError};
use gigcraft_live::{Leader, LeaderEvent, LeaderOut, Snapshot};
use gigcraft_live_net::{LiveServer, NetEvent};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Debug, uniffi::Object)]
pub struct LeaderCore {
    leader: Mutex<Leader>,
    server: Mutex<Option<LiveServer>>,
    /// setlist/index/part last published.
    published: Mutex<Option<String>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[uniffi::export]
impl LeaderCore {
    #[uniffi::constructor]
    pub fn new(name: String) -> Arc<Self> {
        let session = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| format!("{:x}", d.as_nanos()))
            .unwrap_or_else(|_| "session".into());
        let name = if name.trim().is_empty() {
            "Band leader".to_string()
        } else {
            name
        };
        Arc::new(Self {
            leader: Mutex::new(Leader::new(session, name)),
            server: Mutex::new(None),
            published: Mutex::new(None),
        })
    }

    /// Start serving the web viewer and the WebSocket; returns the addresses
    /// band members can open (`http://192.168.1.20:8787/`).
    pub fn start(&self) -> Result<Vec<String>, CoreError> {
        let server = LiveServer::start(gigcraft_live_net::DEFAULT_PORT)
            .map_err(|e| CoreError::new("network", e))?;
        let urls = server.urls();
        *lock(&self.server) = Some(server);
        Ok(urls)
    }

    pub fn port(&self) -> u16 {
        lock(&self.server).as_ref().map_or(0, LiveServer::port)
    }

    /// Network in, show changes out. Returns what happened as JSON:
    /// `[{"type":"joined","name":…}, {"type":"left",…}, {"type":"ready",…},
    /// {"type":"cue","from":…,"text":…}]`.
    pub fn pump(&self, core: Arc<Core>, now_ms: u64) -> String {
        let mut outs = Vec::new();
        let events = lock(&self.server)
            .as_ref()
            .map(LiveServer::poll)
            .unwrap_or_default();
        {
            let mut leader = lock(&self.leader);
            for ev in events {
                match ev {
                    NetEvent::Text { conn, text } => {
                        outs.extend(leader.on_text(conn, &text, now_ms))
                    }
                    NetEvent::Closed { conn } => outs.extend(leader.disconnected(conn)),
                    NetEvent::Connected { .. } => {}
                }
            }
        }
        // Publish the show when it moved.
        let key = {
            let engine = core.lock();
            engine
                .show()
                .map(|s| format!("{}/{}/{}", s.setlist, s.index, s.part))
                .unwrap_or_else(|| "off".into())
        };
        if lock(&self.published).as_deref() != Some(key.as_str()) {
            let snapshot: Option<Snapshot> = core
                .run("perform.snapshot".into(), String::new())
                .ok()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .and_then(|v| serde_json::from_value(v["result"]["snapshot"].clone()).ok());
            outs.extend(lock(&self.leader).set_snapshot(snapshot));
            *lock(&self.published) = Some(key);
        }
        self.carry_out(outs)
    }

    /// A cue from the leader to the band.
    pub fn cue(&self, text: String, now_ms: u64) -> String {
        let outs = lock(&self.leader).cue(&text, now_ms);
        self.carry_out(outs)
    }

    /// The band (`[{"id","name","instrument","ready"}]`).
    pub fn roster_json(&self) -> String {
        serde_json::to_string(&lock(&self.leader).members()).unwrap_or_else(|_| "[]".into())
    }

    /// End the session: tell everyone and stop serving.
    pub fn stop(&self) {
        let outs = lock(&self.leader).stop("The leader ended the session");
        let _ = self.carry_out(outs);
        if let Some(mut s) = lock(&self.server).take() {
            s.stop();
        }
    }
}

impl LeaderCore {
    fn carry_out(&self, outs: Vec<LeaderOut>) -> String {
        let server = lock(&self.server);
        let mut events = Vec::new();
        for out in outs {
            match out {
                LeaderOut::Send { conn, text } => {
                    if let Some(s) = server.as_ref() {
                        s.send(conn, text);
                    }
                }
                LeaderOut::Close { conn } => {
                    if let Some(s) = server.as_ref() {
                        s.close(conn);
                    }
                }
                LeaderOut::Event(ev) => events.push(match ev {
                    LeaderEvent::Joined(m) => serde_json::json!({"type": "joined", "name": m.name, "instrument": m.instrument}),
                    LeaderEvent::Left(m) => serde_json::json!({"type": "left", "name": m.name}),
                    LeaderEvent::ReadyChanged(m) => serde_json::json!({"type": "ready", "name": m.name, "ready": m.ready}),
                    LeaderEvent::Cue(c) => serde_json::json!({"type": "cue", "from": c.from, "text": c.text}),
                    LeaderEvent::Rejected { reason, .. } => serde_json::json!({"type": "rejected", "reason": reason}),
                }),
            }
        }
        serde_json::to_string(&events).unwrap_or_else(|_| "[]".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gigcraft_live::Follower;
    use gigcraft_live_net::{ClientEvent, LiveClient};
    use std::time::{Duration, Instant};

    #[test]
    fn an_ipad_leader_serves_the_running_show() {
        let dir = tempfile::tempdir().unwrap();
        let core =
            Core::open_or_create_demo(dir.path().join("lib").display().to_string(), "Demo".into())
                .unwrap();
        let lists = core.run("setlist.list".into(), String::new()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&lists).unwrap();
        let sl = v["result"][0]["id"].as_str().unwrap().to_string();
        core.run("perform.start".into(), format!(r#"{{"setlist":"{sl}"}}"#))
            .unwrap();

        let leader = LeaderCore::new("Sven".into());
        let urls = leader.start().unwrap();
        let port = leader.port();
        assert!(port > 0);
        assert!(urls.iter().all(|u| u.ends_with(&format!(":{port}/"))));

        let client = LiveClient::connect(&format!("127.0.0.1:{port}"));
        let mut follower = Follower::new("Anna", "Bass");
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut sent_join = false;
        let mut events = String::new();
        while Instant::now() < deadline && follower.snapshot().is_none() {
            for e in client.poll() {
                match e {
                    ClientEvent::Connected if !sent_join => {
                        client.send(follower.join());
                        sent_join = true;
                    }
                    ClientEvent::Text(t) => {
                        follower.on_text(&t).unwrap();
                    }
                    _ => {}
                }
            }
            let ev = leader.pump(core.clone(), 0);
            if ev != "[]" {
                events.push_str(&ev);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let song = follower
            .snapshot()
            .and_then(|s| s.song())
            .map(|s| s.title.clone());
        assert_eq!(song.as_deref(), Some("Harbour Lights"));
        assert!(events.contains(r#""type":"joined""#), "{events}");
        assert!(leader.roster_json().contains("Anna"));
        leader.stop();
    }
}
