use super::*;
use std::io::{BufRead, BufReader};

fn wait_for<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = f() {
            return v;
        }
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn serves_the_viewer_and_404() {
    let mut server = LiveServer::start(0).unwrap();
    let get = |path: &str| {
        let mut s = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut r = BufReader::new(s);
        let mut status = String::new();
        r.read_line(&mut status).unwrap();
        let mut rest = String::new();
        r.read_to_string(&mut rest).unwrap();
        (status, rest)
    };
    let (status, body) = get("/");
    assert!(status.contains("200"), "{status}");
    assert!(body.contains("GigCraft Live"));
    assert!(body.contains("text/html"));
    let (status, _) = get("/../etc/passwd");
    assert!(status.contains("404"));
    server.stop();
}

#[test]
fn websocket_round_trip_with_the_live_protocol() {
    use gigcraft_live::{Follower, Leader, LeaderOut};
    let server = LiveServer::start(0).unwrap();
    let mut leader = Leader::new("s1", "Sven");
    let client = LiveClient::connect(&format!("127.0.0.1:{}", server.port()));
    wait_for(|| {
        client
            .poll()
            .into_iter()
            .find(|e| *e == ClientEvent::Connected)
    });
    let mut follower = Follower::new("Anna", "Bass");
    client.send(follower.join());

    // Leader side: the join arrives; answer through the server.
    let conn = wait_for(|| {
        server.poll().into_iter().find_map(|e| match e {
            NetEvent::Text { conn, text } => Some((conn, text)),
            _ => None,
        })
    });
    for out in leader.on_text(conn.0, &conn.1, 0) {
        if let LeaderOut::Send { conn, text } = out {
            server.send(conn, text);
        }
    }
    assert_eq!(server.connections(), 1);
    // Follower side: welcome, state, roster.
    wait_for(|| {
        for e in client.poll() {
            if let ClientEvent::Text(t) = e {
                follower.on_text(&t).unwrap();
            }
        }
        (follower.joined() && !follower.roster().is_empty()).then_some(())
    });
    assert_eq!(follower.leader(), "Sven");

    // Closing the server ends the client.
    server.close(conn.0);
    wait_for(|| {
        client
            .poll()
            .into_iter()
            .find(|e| matches!(e, ClientEvent::Closed(_)))
    });
    wait_for(|| {
        server
            .poll()
            .into_iter()
            .find(|e| matches!(e, NetEvent::Closed { .. }))
    });
}

#[test]
fn connect_failure_is_reported() {
    // Nothing listens on port 9 of localhost here.
    let client = LiveClient::connect("127.0.0.1:9");
    let ev = wait_for(|| client.poll().into_iter().next());
    assert!(matches!(ev, ClientEvent::Closed(_)));
    let client = LiveClient::connect("https://example.invalid/");
    let ev = wait_for(|| client.poll().into_iter().next());
    assert!(matches!(ev, ClientEvent::Closed(_)));
}

#[test]
fn addresses_people_type() {
    assert_eq!(
        ws_url("http://192.168.1.20:8787/").as_deref(),
        Some("ws://192.168.1.20:8787/ws")
    );
    assert_eq!(
        ws_url("192.168.1.20").as_deref(),
        Some("ws://192.168.1.20:8787/ws")
    );
    assert_eq!(
        ws_url(" studio.local:9000 ").as_deref(),
        Some("ws://studio.local:9000/ws")
    );
    assert_eq!(ws_url(""), None);
    assert_eq!(ws_url("https://x/"), None);
}

#[test]
fn qr_codes_are_square() {
    let q = qr("http://192.168.1.20:8787/").unwrap();
    assert!(q.len() >= 21);
    assert!(q.iter().all(|row| row.len() == q.len()));
    assert!(q[0][0]); // finder pattern corner
    assert!(qr(&"x".repeat(10_000)).is_none());
}

/// Needs multicast on the local network (often blocked in CI containers).
#[test]
#[ignore = "needs mDNS multicast"]
fn finds_itself_on_the_network() {
    let mut server = LiveServer::start(0).unwrap();
    server.announce("Test Leader").unwrap();
    let mut discovery = Discovery::start().unwrap();
    let found = wait_for(|| {
        discovery
            .leaders()
            .into_iter()
            .find(|l| l.name == "Test Leader")
    });
    assert!(found.address.ends_with(&format!(":{}", server.port())));
    server.stop();
}
