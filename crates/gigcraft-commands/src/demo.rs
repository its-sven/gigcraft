//! A small demo library: original songs written for GigCraft (no third-party
//! lyrics), one setlist with two sets and a break.

use crate::{CommandError, Engine};
use serde_json::json;

struct DemoSong {
    chart: &'static str,
    duration: &'static str,
    tags: &'static [&'static str],
    preset: u8,
}

const SONGS: &[DemoSong] = &[
    DemoSong {
        chart: include_str!("../demo/harbour-lights.cho"),
        duration: "3:40",
        tags: &["ballad"],
        preset: 0,
    },
    DemoSong {
        chart: include_str!("../demo/northbound-train.cho"),
        duration: "3:05",
        tags: &["rock", "opener"],
        preset: 1,
    },
    DemoSong {
        chart: include_str!("../demo/lantern-waltz.cho"),
        duration: "2:50",
        tags: &["waltz"],
        preset: 2,
    },
    DemoSong {
        chart: include_str!("../demo/copper-sky.cho"),
        duration: "4:10",
        tags: &["ballad"],
        preset: 0,
    },
    DemoSong {
        chart: include_str!("../demo/last-call-shuffle.cho"),
        duration: "5:00",
        tags: &["blues", "closer"],
        preset: 3,
    },
];

fn directive<'a>(chart: &'a str, name: &str) -> Option<&'a str> {
    chart.lines().find_map(|l| {
        let inner = l.trim().strip_prefix('{')?.strip_suffix('}')?;
        let (k, v) = inner.split_once(':')?;
        (k.trim() == name).then(|| v.trim())
    })
}

/// Fill an (empty) library with the demo songs and setlist.
pub fn populate(engine: &mut Engine) -> Result<(), CommandError> {
    let mut ids = Vec::new();
    for s in SONGS {
        let mut params = json!({
            "title": directive(s.chart, "title").unwrap_or("Demo"),
            "artist": directive(s.chart, "artist").unwrap_or_default(),
            "key": directive(s.chart, "key"),
            "time_signature": directive(s.chart, "time"),
            "duration": s.duration,
            "tags": s.tags,
            "chart": s.chart,
            "parts": [
                { "name": "Song", "actions": [
                    { "device": "Play Acoustic", "type": "program_change", "channel": 1, "program": s.preset }
                ]}
            ],
        });
        if let Some(t) = directive(s.chart, "tempo").and_then(|t| t.parse::<f64>().ok()) {
            params["tempo_bpm"] = json!(t);
        }
        if let Some(c) = directive(s.chart, "capo").and_then(|c| c.parse::<u8>().ok()) {
            params["capo"] = json!(c);
        }
        let out = engine.run("song.create", &params)?;
        ids.push(out.result["id"].as_str().unwrap_or_default().to_string());
    }
    let list = engine.run(
        "setlist.create",
        &json!({"name": "Demo Gig", "date": "2026-12-31", "notes": "Doors 20:00, stage 21:00"}),
    )?;
    let sl = list.result["id"].clone();
    for (i, id) in ids.iter().enumerate().take(4) {
        let notes = if i == 0 {
            "Count in 4, start together"
        } else {
            ""
        };
        engine.run(
            "setlist.add_song",
            &json!({"id": sl, "song": id, "notes": notes}),
        )?;
        if i == 1 {
            engine.run(
                "setlist.add_break",
                &json!({"id": sl, "minutes": 2, "label": "Retune"}),
            )?;
        }
    }
    engine.run("setlist.add_set", &json!({"id": sl, "name": "Encore"}))?;
    if let Some(last) = ids.last() {
        engine.run("setlist.add_song", &json!({"id": sl, "song": last}))?;
    }
    engine.run("setlist.add_song", &json!({"id": sl, "song": ids.first().map(String::as_str).unwrap_or_default(), "optional": true, "key": "A", "notes": "Only if they shout"}))?;
    // Demo content is not something to undo.
    engine.clear_history();
    Ok(())
}
