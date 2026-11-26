use crate::params::Params;
use crate::{CommandError, Engine};
use gigcraft_live::{Slot, Snapshot, SongSlot};
use gigcraft_model::SetlistId;
use gigcraft_perform::{Action, Effect, Item, Show};
use serde_json::{Value, json};

pub(crate) fn run(e: &mut Engine, id: &str, p: &Params<'_>) -> Result<Value, CommandError> {
    match id {
        "perform.start" => {
            let sid: SetlistId = p.id("setlist")?;
            let list = e
                .lib
                .setlist(sid)
                .ok_or_else(|| CommandError::NotFound(format!("no setlist with id {sid}")))?;
            let mut show = Show::new(list, |song| e.lib.song(song));
            let effects = match p.opt_usize("index")? {
                Some(i) => show.goto(i, p.opt_usize("part")?.unwrap_or(0)),
                None => show.start(),
            };
            e.show = Some(show);
            Ok(report(e, &effects))
        }
        "perform.action" => {
            let name = p.str("action")?;
            let action: Action =
                serde_json::from_value(Value::String(name.to_string())).map_err(|_| {
                    let all: Vec<String> = Action::ALL
                        .iter()
                        .filter_map(|a| serde_json::to_value(a).ok())
                        .map(|v| v.to_string())
                        .collect();
                    CommandError::InvalidParams(format!(
                        "unknown action `{name}`; one of {}",
                        all.join(", ")
                    ))
                })?;
            let show = running(e)?;
            let effects = show.apply(action);
            if action == Action::Exit {
                let out = report(e, &effects);
                e.show = None;
                return Ok(out);
            }
            Ok(report(e, &effects))
        }
        "perform.goto" => {
            let index = p.opt_usize("index")?.unwrap_or(0);
            let part = p.opt_usize("part")?.unwrap_or(0);
            let effects = running(e)?.goto(index, part);
            Ok(report(e, &effects))
        }
        "perform.state" => Ok(report(e, &[])),
        "perform.track_position" => {
            // The backing track moved on: switch to the part its section belongs to.
            let secs = p
                .opt_f64("secs")?
                .ok_or_else(|| CommandError::InvalidParams("secs is required".into()))?;
            let current = e
                .show
                .as_ref()
                .ok_or_else(|| {
                    CommandError::Rejected("perform mode is not running (perform.start)".into())
                })?
                .current()
                .cloned();
            let target = match current {
                Some(Item::Song {
                    song, part_names, ..
                }) => e
                    .lib
                    .song(song)
                    .and_then(|s| {
                        s.markers
                            .iter()
                            .filter(|m| m.at_secs <= secs)
                            .max_by(|a, b| a.at_secs.total_cmp(&b.at_secs))
                    })
                    .and_then(|m| gigcraft_perform::part_for_marker(&part_names, &m.name)),
                _ => None,
            };
            let show = running(e)?;
            let effects = match target {
                Some(part) if part != show.part => {
                    let index = show.index;
                    show.goto(index, part)
                }
                _ => Vec::new(),
            };
            Ok(report(e, &effects))
        }
        "perform.snapshot" => Ok(json!({ "snapshot": snapshot(e) })),
        "perform.stop" => {
            e.show = None;
            Ok(json!({ "running": false }))
        }
        _ => Err(CommandError::UnknownCommand(id.to_string())),
    }
}

fn running(e: &mut Engine) -> Result<&mut Show, CommandError> {
    e.show
        .as_mut()
        .ok_or_else(|| CommandError::Rejected("perform mode is not running (perform.start)".into()))
}

/// State plus effects; `send_part` effects carry the part's MIDI actions so a
/// client can send them without another lookup.
fn report(e: &Engine, effects: &[Effect]) -> Value {
    let Some(show) = &e.show else {
        return json!({ "running": false, "effects": effects });
    };
    let effects: Vec<Value> = effects
        .iter()
        .map(|eff| {
            let mut v = serde_json::to_value(eff).unwrap_or(Value::Null);
            if let Effect::SendPart { song, part } = eff {
                let part = e.lib.song(*song).and_then(|s| s.parts.get(*part));
                v["part_name"] = json!(part.map(|p| p.name.clone()));
                v["actions"] =
                    serde_json::to_value(part.map(|p| p.actions.clone()).unwrap_or_default())
                        .unwrap_or(Value::Null);
            }
            v
        })
        .collect();
    let current = show
        .current()
        .map(|i| serde_json::to_value(i).unwrap_or(Value::Null));
    let next = show.up_next().map(|i| match i {
        Item::Song { title, key, .. } => json!({ "type": "song", "title": title, "key": key }),
        Item::Break { label, minutes, .. } => {
            json!({ "type": "break", "label": label, "minutes": minutes })
        }
        Item::Missing { .. } => json!({ "type": "missing" }),
    });
    json!({
        "running": true,
        "setlist": show.setlist.to_string(),
        "name": show.name,
        "index": show.index,
        "part": show.part,
        "count": show.items.len(),
        "current": current,
        "next": next,
        "effects": effects,
    })
}

/// The show for band members (`gigcraft-live`): position, current and next
/// item; the current song with its chart (source and parsed, in the gig key).
pub(crate) fn snapshot(e: &Engine) -> Option<Snapshot> {
    let show = e.show.as_ref()?;
    let slot = |item: &Item, with_chart: bool| match item {
        Item::Song {
            song,
            title,
            artist,
            key,
            tempo_bpm,
            gig_notes,
            part_names,
            number,
            ..
        } => {
            let stored = e.lib.song(*song);
            let song_key = stored.and_then(|s| s.key.clone());
            let semitones = match (song_key.as_deref(), key.as_deref()) {
                (Some(from), Some(to)) => {
                    gigcraft_chordpro::semitones_between(from, to).unwrap_or(0)
                }
                _ => 0,
            };
            let source = if with_chart {
                e.lib.chart(*song).map(str::to_string)
            } else {
                None
            };
            let chart = source
                .as_deref()
                .map(|src| gigcraft_chordpro::parse(src, semitones).chart)
                .and_then(|c| serde_json::to_value(c).ok());
            Slot::Song(SongSlot {
                song: *song,
                title: title.clone(),
                artist: artist.clone(),
                key: key.clone(),
                tempo_bpm: *tempo_bpm,
                time_signature: stored.and_then(|s| s.time_signature.clone()),
                gig_notes: gig_notes.clone(),
                part_names: part_names.clone(),
                number: *number,
                chart_source: source,
                semitones,
                chart,
            })
        }
        Item::Break { label, minutes, .. } => Slot::Break {
            label: label.clone(),
            minutes: *minutes,
        },
        Item::Missing { .. } => Slot::Missing,
    };
    Some(Snapshot {
        setlist: show.setlist,
        setlist_name: show.name.clone(),
        index: show.index,
        count: show.items.len(),
        part: show.part,
        current: show.current().map(|i| slot(i, true)),
        next: show.up_next().map(|i| slot(i, false)),
    })
}
