use super::*;

fn snap(setlist: SetlistId, index: usize, part: usize) -> Snapshot {
    Snapshot {
        setlist,
        setlist_name: "Demo Gig".into(),
        index,
        count: 5,
        part,
        current: Some(Slot::Song(SongSlot {
            song: format!("00000000-0000-0000-0000-{index:012}")
                .parse()
                .unwrap(),
            title: format!("Song {index}"),
            part_names: vec!["Verse".into(), "Chorus".into()],
            number: index + 1,
            ..Default::default()
        })),
        next: Some(Slot::Break {
            label: "Break".into(),
            minutes: 15,
        }),
    }
}

/// Texts sent to `conn`.
fn sent(out: &[LeaderOut], conn: ConnId) -> Vec<ToFollower> {
    out.iter()
        .filter_map(|o| match o {
            LeaderOut::Send { conn: c, text } if *c == conn => {
                Some(serde_json::from_str(text).unwrap())
            }
            _ => None,
        })
        .collect()
}

fn join(leader: &mut Leader, conn: ConnId, name: &str) -> Vec<LeaderOut> {
    leader.on_text(conn, &Follower::new(name, "Bass").join(), 0)
}

#[test]
fn join_gets_welcome_state_and_roster() {
    let mut l = Leader::new("s1", "Sven");
    let list = SetlistId::default();
    l.set_snapshot(Some(snap(list, 0, 0)));
    let out = join(&mut l, 7, "Anna");
    let msgs = sent(&out, 7);
    assert!(matches!(&msgs[0], ToFollower::Welcome { member: 1, leader, .. } if leader == "Sven"));
    assert!(matches!(&msgs[1], ToFollower::State { snapshot: Some(s), .. } if s.index == 0));
    assert!(matches!(&msgs[2], ToFollower::Roster { members } if members.len() == 1));
    // Once the leader has scrolled, a late joiner gets that too.
    let song = l.snapshot().unwrap().song().unwrap().song;
    l.scroll(
        song,
        Spot {
            block: 1,
            fraction: 0.5,
        },
    );
    let msgs = sent(&join(&mut l, 9, "Ben"), 9);
    assert!(matches!(&msgs[2], ToFollower::Scroll { spot, .. } if spot.block == 1));
    assert!(out.contains(&LeaderOut::Event(LeaderEvent::Joined(Member {
        id: 1,
        name: "Anna".into(),
        instrument: "Bass".into(),
        ready: false
    }))));
}

#[test]
fn other_protocol_is_turned_away() {
    let mut l = Leader::new("s1", "Sven");
    let out = l.on_text(3, r#"{"type":"join","protocol":99,"name":"Future"}"#, 0);
    assert!(matches!(&sent(&out, 3)[0], ToFollower::Bye { .. }));
    assert!(out.contains(&LeaderOut::Close { conn: 3 }));
    assert!(l.members().is_empty());
}

#[test]
fn garbage_is_rejected_not_fatal() {
    let mut l = Leader::new("s1", "Sven");
    let out = l.on_text(3, "not json", 0);
    assert!(matches!(
        &out[0],
        LeaderOut::Event(LeaderEvent::Rejected { conn: 3, .. })
    ));
    // Messages before join are ignored.
    assert!(l.on_text(3, r#"{"type":"cue","text":"hi"}"#, 0).is_empty());
}

#[test]
fn names_are_cleaned() {
    let mut l = Leader::new("s1", "Sven");
    l.on_text(
        1,
        r#"{"type":"join","protocol":1,"name":"  \u0007\n  ","instrument":"Drums"}"#,
        0,
    );
    let long = "x".repeat(100);
    l.on_text(2, &Follower::new(&long, "").join(), 0);
    let m = l.members();
    assert_eq!(m[0].name, "Guest");
    assert_eq!(m[1].name.chars().count(), MAX_NAME_CHARS);
}

#[test]
fn ready_resets_when_the_song_changes_but_not_the_part() {
    let mut l = Leader::new("s1", "Sven");
    let list = SetlistId::default();
    l.set_snapshot(Some(snap(list, 0, 0)));
    join(&mut l, 1, "Anna");
    join(&mut l, 2, "Ben");
    let out = l.on_text(1, r#"{"type":"ready","ready":true}"#, 0);
    assert_eq!(l.ready_count(), 1);
    assert!(matches!(&sent(&out, 2)[0], ToFollower::Roster { members } if members[0].ready));

    // Next part: still ready.
    l.set_snapshot(Some(snap(list, 0, 1)));
    assert_eq!(l.ready_count(), 1);
    // Next song: everyone not ready, roster goes out.
    let out = l.set_snapshot(Some(snap(list, 1, 0)));
    assert_eq!(l.ready_count(), 0);
    let to_ben = sent(&out, 2);
    assert!(matches!(&to_ben[0], ToFollower::State { seq: 3, .. }));
    assert!(matches!(&to_ben[1], ToFollower::Roster { .. }));
    // The same snapshot again sends nothing.
    assert!(l.set_snapshot(Some(snap(list, 1, 0))).is_empty());
}

#[test]
fn cues_reach_everyone_and_history_is_bounded() {
    let mut l = Leader::new("s1", "Sven");
    join(&mut l, 1, "Anna");
    join(&mut l, 2, "Ben");
    let out = l.on_text(1, r#"{"type":"cue","text":"  last chorus  "}"#, 42);
    let cue = Cue {
        from: "Anna".into(),
        text: "last chorus".into(),
        at_ms: 42,
    };
    assert_eq!(sent(&out, 2), vec![ToFollower::Cue(cue.clone())]);
    assert_eq!(sent(&out, 1), vec![ToFollower::Cue(cue)]);
    assert!(l.on_text(1, r#"{"type":"cue","text":"   "}"#, 0).is_empty());
    for i in 0..30 {
        l.cue(&format!("cue {i}"), i);
    }
    assert_eq!(l.cues().len(), CUE_HISTORY);
    assert_eq!(l.cues()[CUE_HISTORY - 1].text, "cue 29");
    // A late joiner gets the history.
    let out = join(&mut l, 3, "Cem");
    assert!(
        matches!(&sent(&out, 3)[0], ToFollower::Welcome { cues, .. } if cues.len() == CUE_HISTORY)
    );
}

#[test]
fn leaving_updates_the_roster() {
    let mut l = Leader::new("s1", "Sven");
    join(&mut l, 1, "Anna");
    join(&mut l, 2, "Ben");
    let out = l.disconnected(1);
    assert!(matches!(&sent(&out, 2)[0], ToFollower::Roster { members } if members.len() == 1));
    assert!(l.disconnected(1).is_empty());
    let out = l.stop("gig over");
    assert!(matches!(&sent(&out, 2)[0], ToFollower::Bye { reason } if reason == "gig over"));
    assert!(out.contains(&LeaderOut::Close { conn: 2 }));
}

#[test]
fn follower_tracks_the_leader() {
    let mut l = Leader::new("s1", "Sven");
    let list = SetlistId::default();
    l.set_snapshot(Some(snap(list, 0, 0)));
    let mut f = Follower::new("Anna", "Bass");
    let mut events = Vec::new();
    for msg in sent(&l.on_text(1, &f.join(), 0), 1) {
        events.extend(f.on_message(msg));
    }
    assert!(f.joined());
    assert_eq!(f.leader(), "Sven");
    assert_eq!(
        events[1],
        FollowerEvent::Moved {
            song_changed: true,
            part_changed: true
        }
    );
    // Ready round trip.
    let text = f.set_ready(true);
    for msg in sent(&l.on_text(1, &text, 0), 1) {
        f.on_message(msg);
    }
    assert!(f.ready());
    assert!(f.roster()[0].ready);
    // Part change only.
    let mut ev = Vec::new();
    for msg in sent(&l.set_snapshot(Some(snap(list, 0, 1))), 1) {
        ev.extend(f.on_message(msg));
    }
    assert_eq!(
        ev,
        vec![FollowerEvent::Moved {
            song_changed: false,
            part_changed: true
        }]
    );
    assert!(f.ready());
    // Song change: not ready any more.
    for msg in sent(&l.set_snapshot(Some(snap(list, 1, 0))), 1) {
        f.on_message(msg);
    }
    assert!(!f.ready());
    assert_eq!(
        f.snapshot()
            .and_then(|s| s.song())
            .map(|s| s.title.as_str()),
        Some("Song 1")
    );
    // Stale state is ignored.
    assert!(
        f.on_message(ToFollower::State {
            seq: 1,
            snapshot: None
        })
        .is_empty()
    );
    // Bye ends it.
    let ev = f.on_message(ToFollower::Bye {
        reason: "gig over".into(),
    });
    assert_eq!(
        ev,
        vec![FollowerEvent::Ended {
            reason: "gig over".into()
        }]
    );
    assert!(!f.joined());
}

#[test]
fn wire_format_is_stable() {
    let text = encode(&ToLeader::Ready { ready: true });
    assert_eq!(text, r#"{"type":"ready","ready":true}"#);
    let msg: ToFollower = serde_json::from_str(
        r#"{"type":"state","seq":3,"snapshot":{"setlist":"00000000-0000-0000-0000-000000000001","setlist_name":"Gig","index":2,"count":9,"part":0,
            "current":{"type":"break","label":"Pause","minutes":10,"future_field":1},"next":null,"also_new":true}}"#,
    )
    .unwrap();
    assert!(matches!(
        msg,
        ToFollower::State {
            seq: 3,
            snapshot: Some(Snapshot {
                current: Some(Slot::Break { minutes: 10, .. }),
                ..
            })
        }
    ));
    let cue: ToFollower =
        serde_json::from_str(r#"{"type":"cue","from":"Ben","text":"stop"}"#).unwrap();
    assert!(matches!(cue, ToFollower::Cue(Cue { at_ms: 0, .. })));
}

#[test]
fn a_spot_is_the_same_place_on_any_screen() {
    // Small screen: blocks at 0, 100, 300; chart 400 high.
    let small = [0.0, 100.0, 300.0];
    let spot = Spot::at(200.0, &small, 400.0);
    assert_eq!(spot.block, 1);
    assert!((spot.fraction - 0.5).abs() < 1e-6);
    // Larger type: the same place is further down.
    let big = [0.0, 150.0, 450.0];
    assert_eq!(spot.offset(&big, 600.0), Some(300.0));
    assert_eq!(Spot::at(-5.0, &small, 400.0), Spot::default());
    assert_eq!(
        Spot {
            block: 9,
            fraction: 0.0
        }
        .offset(&small, 400.0),
        None
    );
}

#[test]
fn followers_see_the_leaders_scrolling_and_the_band_mark() {
    let mut l = Leader::new("s1", "Sven");
    let list = SetlistId::default();
    l.set_snapshot(Some(snap(list, 0, 0)));
    let mut f = Follower::new("Anna", "Bass");
    for o in join(&mut l, 7, "Anna") {
        if let LeaderOut::Send { text, .. } = o {
            f.on_text(&text).unwrap();
        }
    }
    let song = l.snapshot().unwrap().song().unwrap().song;
    // Scrolling: sent once, not again for a tiny move.
    let out = l.scroll(
        song,
        Spot {
            block: 2,
            fraction: 0.3,
        },
    );
    assert_eq!(sent(&out, 7).len(), 1);
    assert!(
        l.scroll(
            song,
            Spot {
                block: 2,
                fraction: 0.31
            }
        )
        .is_empty()
    );
    for o in out {
        if let LeaderOut::Send { text, .. } = o {
            assert_eq!(f.on_text(&text).unwrap(), vec![FollowerEvent::Scrolled]);
        }
    }
    assert_eq!(f.leader_scroll().unwrap().1.block, 2);
    // A follower marks a place: everyone gets it, with who set it.
    let text = f.mark_spot(
        song,
        Spot {
            block: 1,
            fraction: 0.0,
        },
        "from the bridge",
    );
    let out = l.on_text(7, &text, 0);
    let mark = l.mark().unwrap().clone();
    assert_eq!(mark.from, "Anna");
    assert_eq!(mark.text, "from the bridge");
    assert!(out.contains(&LeaderOut::Event(LeaderEvent::Mark(Some(mark.clone())))));
    // A late joiner gets it with the welcome.
    let msgs = sent(&join(&mut l, 8, "Ben"), 8);
    assert!(
        matches!(&msgs[0], ToFollower::Welcome { mark: Some(m), .. } if m.text == "from the bridge")
    );
    // The leader clears it.
    let out = l.set_mark(None);
    assert!(matches!(&sent(&out, 7)[0], ToFollower::Mark { mark: None }));
    assert!(l.mark().is_none());
    // Nonsense is ignored.
    assert!(
        l.scroll(
            song,
            Spot {
                block: 1,
                fraction: f32::NAN
            }
        )
        .is_empty()
    );
}
