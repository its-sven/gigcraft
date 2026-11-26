use super::*;
use serde_json::json;

fn engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib = Library::create(dir.path().join("lib"), "Test").expect("create");
    (dir, Engine::new(lib))
}

fn run(e: &mut Engine, id: &str, params: Value) -> Outcome {
    e.run(id, &params)
        .unwrap_or_else(|err| panic!("{id}: {err}"))
}

fn new_song(e: &mut Engine, title: &str, duration: &str) -> String {
    let out = run(
        e,
        "song.create",
        json!({"title": title, "duration": duration, "key": "G", "chart": "[G]La [C]la"}),
    );
    out.result["id"].as_str().expect("id").to_string()
}

#[test]
fn every_dispatched_command_has_a_spec_and_vice_versa() {
    let (_d, mut e) = engine();
    for spec in specs() {
        if spec.id == "commands.list" || spec.id.starts_with("edit.") {
            continue;
        }
        let err = e.run(spec.id, &json!({})).err();
        assert!(
            !matches!(err, Some(CommandError::UnknownCommand(_))),
            "{} not dispatched",
            spec.id
        );
    }
    assert!(matches!(
        e.run("song.nope", &Value::Null),
        Err(CommandError::UnknownCommand(_))
    ));
    assert!(matches!(
        e.run("song.list", &json!([1])),
        Err(CommandError::InvalidParams(_))
    ));
}

#[test]
fn song_create_update_chart_trash_with_undo() {
    let (_d, mut e) = engine();
    let id = new_song(&mut e, "Harbour Lights", "3:30");
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["duration_secs"], 210);
    assert_eq!(got.result["chart"], "[G]La [C]la");

    let upd = run(
        &mut e,
        "song.update",
        json!({"id": id, "fields": {"artist": "Demo", "key": null}}),
    );
    assert_eq!(upd.changes[0].action, "updated");
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["artist"], "Demo");
    assert!(got.result["song"].get("key").is_none());
    assert_eq!(
        got.result["chart"], "[G]La [C]la",
        "chart kept when not given"
    );

    let chart = run(&mut e, "song.chart", json!({"id": id, "transpose": 2}));
    assert_eq!(
        chart.result["chart"]["blocks"][0]["lines"][0]["segments"][0]["chord"],
        "A"
    );

    assert!(matches!(
        e.run("song.update", &json!({"id": id, "fields": {"id": "x"}})),
        Err(CommandError::InvalidParams(_))
    ));
    assert!(matches!(
        e.run("song.update", &json!({"id": id, "fields": {"title": " "}})),
        Err(CommandError::Rejected(_))
    ));

    run(&mut e, "song.trash", json!({"id": id}));
    assert_eq!(
        run(&mut e, "song.list", Value::Null)
            .result
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    run(&mut e, "edit.undo", Value::Null);
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["artist"], "Demo");
    assert_eq!(got.result["chart"], "[G]La [C]la");
    run(&mut e, "edit.undo", Value::Null); // the update
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["key"], "G");
    run(&mut e, "edit.redo", Value::Null);
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["artist"], "Demo");
}

#[test]
fn undo_of_create_trashes_and_survives_reopen() {
    let (_d, mut e) = engine();
    let id = new_song(&mut e, "Temp", "1:00");
    let undone = run(&mut e, "edit.undo", Value::Null);
    assert_eq!(undone.changes[0].action, "trashed");
    let root = e.library().root().to_path_buf();
    let lib = Library::open(&root).expect("open");
    assert!(lib.song(id.parse().expect("id")).is_none());
    assert!(root.join(gigcraft_store::TRASH_DIR).is_dir());
    assert!(matches!(
        e.run("edit.undo", &Value::Null),
        Err(CommandError::Rejected(_))
    ));
}

#[test]
fn setlist_flow() {
    let (_d, mut e) = engine();
    let a = new_song(&mut e, "Alpha", "3:00");
    let b = new_song(&mut e, "Bravo", "4:00");
    let out = run(
        &mut e,
        "setlist.create",
        json!({"name": "Summer Fest", "date": "2026-07-04"}),
    );
    let sl = out.result["id"].as_str().expect("id").to_string();
    run(&mut e, "setlist.add_song", json!({"id": sl, "song": a}));
    run(
        &mut e,
        "setlist.add_break",
        json!({"id": sl, "minutes": 10}),
    );
    run(
        &mut e,
        "setlist.add_song",
        json!({"id": sl, "song": b, "key": "A"}),
    );
    run(
        &mut e,
        "setlist.add_set",
        json!({"id": sl, "name": "Encore"}),
    );
    run(
        &mut e,
        "setlist.add_song",
        json!({"id": sl, "song": a, "optional": true}),
    );
    run(
        &mut e,
        "setlist.move_entry",
        json!({"id": sl, "from_set": 0, "from": 2, "to": 0}),
    );

    let got = run(&mut e, "setlist.get", json!({"id": sl})).result;
    assert_eq!(got["sets"][0]["entries"][0]["title"], "Bravo");
    assert_eq!(got["sets"][0]["entries"][0]["key"], "A");
    assert_eq!(
        got["sets"][0]["entries"][1]["key"], "G",
        "song key when no override"
    );
    assert_eq!(got["sets"][0]["entries"][2]["type"], "break");
    assert_eq!(got["sets"][0]["duration"], "17:00");
    assert_eq!(got["total"], "17:00");
    assert_eq!(got["optional"], "3:00");

    // A trashed song stays in the setlist, marked missing.
    run(&mut e, "song.trash", json!({"id": b}));
    let got = run(&mut e, "setlist.get", json!({"id": sl})).result;
    assert_eq!(got["sets"][0]["entries"][0]["missing"], true);

    assert!(matches!(
        e.run("setlist.add_song", &json!({"id": sl, "song": b})),
        Err(CommandError::NotFound(_))
    ));
    assert!(matches!(
        e.run(
            "setlist.remove_entry",
            &json!({"id": sl, "set": 5, "index": 0})
        ),
        Err(CommandError::InvalidParams(_))
    ));
    let listed = run(&mut e, "setlist.list", Value::Null).result;
    assert_eq!(listed[0]["songs"], 3, "breaks are not songs");
}

#[test]
fn demo_library_is_valid() {
    let (_d, mut e) = engine();
    demo::populate(&mut e).expect("demo");
    assert_eq!(e.library().songs().len(), 5);
    assert!(e.undo_label().is_none());
    for song in e.library().songs() {
        use gigcraft_model::Validate;
        assert!(
            song.issues().is_empty(),
            "{}: {:?}",
            song.title,
            song.issues()
        );
        let parsed = gigcraft_chordpro::parse(e.library().chart(song.id).unwrap_or_default(), 0);
        assert!(
            parsed.warnings.is_empty(),
            "{}: {:?}",
            song.title,
            parsed.warnings
        );
        assert!(!parsed.chart.blocks.is_empty());
    }
    let lists = run(&mut e, "setlist.list", Value::Null).result;
    let detail = run(&mut e, "setlist.get", json!({"id": lists[0]["id"]})).result;
    assert_eq!(detail["sets"].as_array().map(Vec::len), Some(2));
    assert_eq!(detail["total"], "20:45");
}

#[test]
fn perform_flow_and_entry_updates() {
    let (_d, mut e) = engine();
    demo::populate(&mut e).expect("demo");
    let lists = run(&mut e, "setlist.list", Value::Null).result;
    let sl = lists[0]["id"].as_str().expect("id").to_string();

    run(
        &mut e,
        "setlist.update_entry",
        json!({"id": sl, "set": 0, "index": 0, "fields": {"key": "A", "notes": "capo 2"}}),
    );
    run(
        &mut e,
        "setlist.update_entry",
        json!({"id": sl, "set": 0, "index": 2, "fields": {"minutes": 5}}),
    );
    assert!(matches!(
        e.run(
            "setlist.update_entry",
            &json!({"id": sl, "set": 0, "index": 2, "fields": {"key": "A"}})
        ),
        Err(CommandError::InvalidParams(_))
    ));

    assert!(matches!(
        e.run("perform.action", &json!({"action": "next"})),
        Err(CommandError::Rejected(_))
    ));
    let st = run(&mut e, "perform.start", json!({"setlist": sl})).result;
    assert_eq!(st["current"]["title"], "Harbour Lights");
    assert_eq!(st["current"]["key"], "A");
    assert_eq!(st["current"]["gig_notes"], "capo 2");
    assert_eq!(st["effects"][1]["type"], "send_part");
    assert_eq!(st["effects"][1]["actions"][0]["device"], "Play Acoustic");
    assert_eq!(st["next"]["title"], "Northbound Train");

    let snap = run(&mut e, "perform.snapshot", Value::Null).result["snapshot"].clone();
    let snap: gigcraft_live::Snapshot = serde_json::from_value(snap).expect("snapshot");
    let song = snap.song().expect("current song");
    assert_eq!(song.title, "Harbour Lights");
    assert_eq!(song.key.as_deref(), Some("A"));
    assert_eq!(song.semitones, 2);
    assert!(
        song.chart_source
            .as_deref()
            .is_some_and(|c| c.contains("[G]"))
    );
    // The parsed chart is in the gig key (G → A).
    assert!(
        song.chart
            .as_ref()
            .is_some_and(|c| c.to_string().contains("\"A\""))
    );
    assert!(matches!(snap.next, Some(gigcraft_live::Slot::Song(ref n)) if n.chart.is_none()));
    let st = run(&mut e, "perform.action", json!({"action": "next"})).result;
    assert_eq!(st["index"], 1);
    let st = run(&mut e, "perform.action", json!({"action": "next"})).result;
    assert_eq!(st["effects"][1], json!({"type": "break", "minutes": 5}));
    assert!(matches!(
        e.run("perform.action", &json!({"action": "dance"})),
        Err(CommandError::InvalidParams(_))
    ));
    let st = run(&mut e, "perform.action", json!({"action": "exit"})).result;
    assert_eq!(st["effects"][0]["type"], "exit");
    assert_eq!(
        run(&mut e, "perform.state", Value::Null).result["running"],
        false
    );
    assert!(e.undo_label().is_some(), "entry updates are undoable");
}

#[test]
fn attach_and_detach() {
    let (d, mut e) = engine();
    let id = new_song(&mut e, "Sheet", "1:00");
    let src = d.path().join("lead sheet.pdf");
    std::fs::write(&src, b"%PDF-1.4").expect("write");
    let out = run(
        &mut e,
        "song.attach",
        json!({"id": id, "path": src.display().to_string()}),
    );
    assert_eq!(out.result["file"], "lead-sheet.pdf");
    let got = run(&mut e, "song.get", json!({"id": id})).result;
    assert_eq!(got["attachments"][0]["kind"], "pdf");
    assert!(
        got["attachments"][0]["path"]
            .as_str()
            .is_some_and(|p| std::path::Path::new(p).is_file())
    );
    run(
        &mut e,
        "song.detach",
        json!({"id": id, "file": "lead-sheet.pdf"}),
    );
    let got = run(&mut e, "song.get", json!({"id": id})).result;
    assert_eq!(got["attachments"].as_array().map(Vec::len), Some(0));
    assert!(matches!(
        e.run("song.detach", &json!({"id": id, "file": "x.pdf"})),
        Err(CommandError::NotFound(_))
    ));
    assert!(
        e.run(
            "song.attach",
            &json!({"id": id, "path": "/does/not/exist.pdf"})
        )
        .is_err()
    );
}

#[test]
fn import_and_export() {
    let (d, mut e) = engine();
    let file = d.path().join("Morning Tide.onsong");
    std::fs::write(
        &file,
        "Morning Tide\nThe Demo Band\nKey: D\n\nVerse 1:\nD        G\nWaves on the stones\n",
    )
    .expect("write");
    let out = run(
        &mut e,
        "import.file",
        json!({"path": file.display().to_string()}),
    )
    .result;
    assert_eq!(out["songs_created"], 1);
    let id = out["first_song"].as_str().expect("id").to_string();
    let chart = run(&mut e, "song.chart", json!({"id": id})).result;
    assert_eq!(
        chart["chart"]["blocks"][0]["lines"][0]["segments"][0]["chord"],
        "D"
    );
    let cho = run(&mut e, "export.chordpro", json!({"id": id})).result;
    assert!(
        cho["text"]
            .as_str()
            .is_some_and(|t| t.contains("{title: Morning Tide}"))
    );
    // One undo step removes the whole import.
    run(&mut e, "edit.undo", Value::Null);
    assert_eq!(e.library().songs().len(), 0);

    let ug = run(&mut e, "import.ultimate_guitar", json!({"links": "see https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-123 and again https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-123?x=1"})).result;
    assert_eq!(ug["created"].as_array().map(Vec::len), Some(1));
    let tsv = run(&mut e, "export.bandhelper_tsv", Value::Null).result;
    assert_eq!(tsv["songs"], 1);
    assert!(matches!(
        e.run(
            "import.file",
            &json!({"path": d.path().display().to_string()})
        ),
        Err(CommandError::Rejected(_))
    ));
}

#[test]
fn song_update_sets_audio_settings_with_undo() {
    let (_d, mut e) = engine();
    let id = run(
        &mut e,
        "song.create",
        json!({"title": "Backing", "tempo_bpm": 120, "time_signature": "4/4",
               "click": {"count_in_bars": 1, "bus": 1}}),
    )
    .result["id"]
        .as_str()
        .expect("id")
        .to_string();
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["click"]["bus"], 1);
    assert_eq!(got.result["song"]["click"]["enabled"], true);

    let upd = run(
        &mut e,
        "song.update",
        json!({"id": id, "fields": {
            "tracks": [{"file": "band.wav", "role": "stem", "gain_db": -3.0, "pan": 0.5, "bus": 0}],
            "markers": [{"name": "Chorus", "at_secs": 30.5, "end_secs": 45.0}],
        }}),
    );
    // The file is not attached yet: saved, but reported.
    assert_eq!(upd.result["issues"][0]["field"], "tracks[0].file");
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["tracks"][0]["role"], "stem");
    assert_eq!(got.result["song"]["markers"][0]["end_secs"], 45.0);
    assert!(got.result["song"].get("extra").is_none());

    assert!(matches!(
        e.run(
            "song.update",
            &json!({"id": id, "fields": {"tracks": [{"role": "stem"}]}})
        ),
        Err(CommandError::InvalidParams(_))
    ));

    run(
        &mut e,
        "song.update",
        json!({"id": id, "fields": {"click": null}}),
    );
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert!(got.result["song"].get("click").is_none());

    run(&mut e, "edit.undo", Value::Null);
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert_eq!(got.result["song"]["click"]["count_in_bars"], 1);
    run(&mut e, "edit.undo", Value::Null);
    let got = run(&mut e, "song.get", json!({"id": id}));
    assert!(got.result["song"].get("tracks").is_none());
    assert!(got.result["song"].get("markers").is_none());
}

#[test]
fn the_backing_track_switches_parts() {
    let (_d, mut e) = engine();
    let id = new_song(&mut e, "Tracked", "3:00");
    run(
        &mut e,
        "song.update",
        json!({"id": id, "fields": {
            "parts": [{"name": "Verse"}, {"name": "Chorus"}],
            "markers": [{"name": "Verse: 1", "at_secs": 0.0}, {"name": "Chorus", "at_secs": 30.0}, {"name": "Verse: 2", "at_secs": 60.0}],
        }}),
    );
    let sl = run(&mut e, "setlist.create", json!({"name": "T"})).result["id"].clone();
    run(&mut e, "setlist.add_song", json!({"id": sl, "song": id}));
    run(&mut e, "perform.start", json!({"setlist": sl}));
    // At 10 s still the verse: nothing to do.
    let r = run(&mut e, "perform.track_position", json!({"secs": 10.0})).result;
    assert_eq!(r["part"], 0);
    assert_eq!(r["effects"].as_array().map(Vec::len), Some(0));
    // At 31 s the chorus: its presets go out.
    let r = run(&mut e, "perform.track_position", json!({"secs": 31.0})).result;
    assert_eq!(r["part"], 1);
    assert!(
        r["effects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "send_part")
    );
    // Again at 32 s: already there.
    let r = run(&mut e, "perform.track_position", json!({"secs": 32.0})).result;
    assert_eq!(r["effects"].as_array().map(Vec::len), Some(0));
    // "Verse: 2" belongs to the part "Verse".
    let r = run(&mut e, "perform.track_position", json!({"secs": 61.0})).result;
    assert_eq!(r["part"], 0);
    assert!(matches!(
        e.run("perform.track_position", &json!({})),
        Err(CommandError::InvalidParams(_))
    ));
}

#[test]
fn an_ultimate_guitar_playlist_becomes_a_setlist() {
    let (_d, mut e) = engine();
    // One song is already in the library with its link (keeps its id).
    run(
        &mut e,
        "import.ultimate_guitar",
        json!({"links": "https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-10"}),
    );
    let data = r#"{"store":{"page":{"data":{"playlist":{"name":"Friday Gig","items":[
        {"id":1,"tab":{"song_name":"Harbour Lights","artist_name":"The Demo Band","tab_url":"https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-10"}},
        {"id":2,"tab":{"song_name":"Night Ferry","artist_name":"Anna Beispiel","tab_url":"https://tabs.ultimate-guitar.com/tab/anna-beispiel/night-ferry-chords-11"}}
    ]}}}}}"#;
    let html = format!(
        "<html><head><title>x</title></head><body><div class=\"js-store\" data-content=\"{}\"></div></body></html>",
        data.replace('&', "&amp;").replace('"', "&quot;")
    );
    let r = run(
        &mut e,
        "import.ultimate_guitar_playlist",
        json!({"html": html, "url": "https://www.ultimate-guitar.com/user/playlist/shared?h=abc"}),
    )
    .result;
    assert_eq!(r["name"], "Friday Gig");
    assert_eq!(r["songs"], 2);
    assert_eq!(r["created"], 1);
    assert_eq!(r["existing"], 1);
    let sl = run(&mut e, "setlist.get", json!({"id": r["setlist"]})).result;
    let titles: Vec<&str> = sl["sets"][0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["Harbour Lights", "Night Ferry"]);
    assert!(sl["notes"].as_str().unwrap().contains("playlist/shared"));
    // One undo removes the whole import.
    run(&mut e, "edit.undo", Value::Null);
    assert!(e.run("setlist.get", &json!({"id": r["setlist"]})).is_err());
    // An empty page is an error, not an empty setlist.
    assert!(matches!(
        e.run(
            "import.ultimate_guitar_playlist",
            &json!({"html": "<html></html>"})
        ),
        Err(CommandError::InvalidParams(_))
    ));
}

fn ug_page(data: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><title>x</title></head><body><div class=\"js-store\" data-content=\"{}\"></div></body></html>",
        data.replace('&', "&amp;").replace('"', "&quot;")
    )
}

#[test]
fn one_import_takes_links_shortcuts_and_pages() {
    let (d, mut e) = engine();
    let tab = "https://tabs.ultimate-guitar.com/tab/the-demo-band/harbour-lights-chords-10";
    // A share message pasted as text: one song, the link kept, its page to load.
    let r = run(
        &mut e,
        "import.text",
        json!({"text": format!("Harbour Lights chords by The Demo Band on Ultimate Guitar\n{tab}")}),
    )
    .result;
    assert_eq!(r["kind"], "links");
    let id = r["first_song"].as_str().unwrap().to_string();
    assert_eq!(r["fetch"][0]["url"], tab);
    let song = run(&mut e, "song.get", json!({"id": id})).result["song"].clone();
    assert_eq!(song["links"][0]["url"], tab);

    // The page fills in what the link cannot say.
    let data = r#"{"store":{"page":{"data":{
        "tab":{"song_name":"Harbour Lights (Live)","artist_name":"The Demo Band","type":"Chords","tonality_name":"Em"},
        "tab_view":{"meta":{"capo":3,"tuning":{"name":"Half Step Down"}}}}}}}"#;
    let r = run(
        &mut e,
        "import.ultimate_guitar_details",
        json!({"id": id, "html": ug_page(data)}),
    )
    .result;
    assert_eq!(r["changed"], true);
    let song = run(&mut e, "song.get", json!({"id": id})).result["song"].clone();
    assert_eq!(song["title"], "Harbour Lights (Live)");
    assert_eq!(song["key"], "Em");
    assert_eq!(song["capo"], 3);
    assert_eq!(song["custom"]["Tuning"], "Half Step Down");
    assert_eq!(song["links"][0]["label"], "Ultimate Guitar (Chords)");

    // A title the user typed stays.
    run(
        &mut e,
        "song.update",
        json!({"id": id, "fields": {"title": "Harbour"}}),
    );
    run(
        &mut e,
        "import.ultimate_guitar_details",
        json!({"id": id, "html": ug_page(data)}),
    );
    assert_eq!(
        run(&mut e, "song.get", json!({"id": id})).result["song"]["title"],
        "Harbour"
    );

    // A .webloc for the same tab finds the song instead of adding it again.
    let webloc = d.path().join("Harbour Lights.webloc");
    std::fs::write(
        &webloc,
        format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict><key>URL</key><string>{tab}</string></dict></plist>"),
    )
    .unwrap();
    let r = run(
        &mut e,
        "import.file",
        json!({"path": webloc.display().to_string()}),
    )
    .result;
    assert_eq!(r["existing"][0], id.as_str());
    assert_eq!(e.library().songs().len(), 1);

    // A playlist link: the app loads the page, nothing changes yet.
    let r = run(
        &mut e,
        "import.text",
        json!({"text": "https://www.ultimate-guitar.com/user/playlist/shared?h=abc"}),
    )
    .result;
    assert_eq!(r["kind"], "fetch_playlist");

    // A saved playlist page becomes a setlist.
    let list = r#"{"store":{"page":{"data":{"playlist":{"name":"Friday","items":[
        {"id":2,"tab":{"song_name":"Night Ferry","artist_name":"Anna Beispiel","tab_url":"https://tabs.ultimate-guitar.com/tab/anna-beispiel/night-ferry-chords-11"}}]}}}}}"#;
    let html = d.path().join("Friday.html");
    std::fs::write(&html, ug_page(list)).unwrap();
    let r = run(
        &mut e,
        "import.file",
        json!({"path": html.display().to_string()}),
    )
    .result;
    assert_eq!(r["kind"], "playlist");
    assert_eq!(r["created"], 1);

    // A file nobody can read says what can be read.
    let pdf = d.path().join("x.bin");
    std::fs::write(&pdf, [0u8, 159, 146, 150, 1, 2, 3]).unwrap();
    let err = e
        .run("import.file", &json!({"path": pdf.display().to_string()}))
        .unwrap_err();
    assert!(err.to_string().contains("ChordPro"), "{err}");
}
