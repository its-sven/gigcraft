use crate::params::Params;
use crate::{CommandError, Engine, Tx};
use gigcraft_model::{
    Entry, Set, Setlist, SetlistId, SongEntry, SongId, Validate, format_duration,
};
use serde_json::{Value, json};

pub(crate) fn run(
    e: &mut Engine,
    id: &str,
    p: &Params<'_>,
    tx: &mut Tx,
) -> Result<Value, CommandError> {
    match id {
        "setlist.list" => Ok(Value::Array(
            e.lib
                .setlists()
                .into_iter()
                .map(|s| summary(e, s))
                .collect(),
        )),
        "setlist.get" => {
            let list = find(e, p.id("id")?)?;
            Ok(detail(e, list))
        }
        "setlist.create" => {
            let mut list = Setlist::new(p.str("name")?.trim());
            list.date = p.opt_str("date")?.map(str::to_string);
            list.notes = p.opt_str("notes")?.unwrap_or_default().to_string();
            save(e, tx, list)
        }
        "setlist.update" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let fields = p.opt_object("fields")?.cloned().unwrap_or_default();
            for (k, v) in &fields {
                match (k.as_str(), v) {
                    ("name", Value::String(s)) => list.name = s.trim().to_string(),
                    ("date", Value::String(s)) => list.date = Some(s.clone()),
                    ("date", Value::Null) => list.date = None,
                    ("notes", Value::String(s)) => list.notes = s.clone(),
                    ("notes", Value::Null) => list.notes.clear(),
                    _ => {
                        return Err(CommandError::InvalidParams(format!(
                            "`{k}` cannot be set here (name, date, notes)"
                        )));
                    }
                }
            }
            save(e, tx, list)
        }
        "setlist.trash" => {
            let sid: SetlistId = p.id("id")?;
            find(e, sid)?;
            e.remove_setlist(tx, sid)?;
            Ok(json!({ "id": sid.to_string() }))
        }
        "setlist.add_set" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let name = p
                .opt_str("name")?
                .map_or_else(|| format!("Set {}", list.sets.len() + 1), str::to_string);
            list.sets.push(Set {
                name,
                entries: Vec::new(),
            });
            save(e, tx, list)
        }
        "setlist.rename_set" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let si = set_index(&list, p.opt_usize("set")?.unwrap_or(0))?;
            list.sets[si].name = p.str("name")?.trim().to_string();
            save(e, tx, list)
        }
        "setlist.remove_set" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let si = set_index(&list, p.opt_usize("set")?.unwrap_or(0))?;
            if list.sets.len() == 1 {
                return Err(CommandError::Rejected(
                    "a setlist keeps at least one set".into(),
                ));
            }
            list.sets.remove(si);
            save(e, tx, list)
        }
        "setlist.add_song" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let song: SongId = p.id("song")?;
            if e.lib.song(song).is_none() {
                return Err(CommandError::NotFound(format!("no song with id {song}")));
            }
            let entry = SongEntry {
                song,
                key: p.opt_str("key")?.map(str::to_string),
                tempo_bpm: p.opt_f64("tempo_bpm")?,
                notes: p.opt_str("notes")?.unwrap_or_default().to_string(),
                optional: p.opt_bool("optional")?.unwrap_or(false),
            };
            insert(&mut list, p, Entry::Song(entry))?;
            save(e, tx, list)
        }
        "setlist.add_break" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let minutes = p.opt_u32("minutes")?.unwrap_or(15);
            let label = p.opt_str("label")?.unwrap_or_default().to_string();
            insert(&mut list, p, Entry::Break { label, minutes })?;
            save(e, tx, list)
        }
        "setlist.move_entry" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let from_set = set_index(&list, p.opt_usize("from_set")?.unwrap_or(0))?;
            let from = entry_index(&list, from_set, p.opt_usize("from")?.unwrap_or(0))?;
            let to_set = set_index(&list, p.opt_usize("to_set")?.unwrap_or(from_set))?;
            let entry = list.sets[from_set].entries.remove(from);
            let len = list.sets[to_set].entries.len();
            let to = p.opt_usize("to")?.unwrap_or(len).min(len);
            list.sets[to_set].entries.insert(to, entry);
            save(e, tx, list)
        }
        "setlist.remove_entry" => {
            let mut list = find(e, p.id("id")?)?.clone();
            let si = set_index(&list, p.opt_usize("set")?.unwrap_or(0))?;
            let ei = entry_index(&list, si, p.opt_usize("index")?.unwrap_or(0))?;
            list.sets[si].entries.remove(ei);
            save(e, tx, list)
        }
        _ => Err(CommandError::UnknownCommand(id.to_string())),
    }
}

fn find(e: &Engine, id: SetlistId) -> Result<&Setlist, CommandError> {
    e.lib
        .setlist(id)
        .ok_or_else(|| CommandError::NotFound(format!("no setlist with id {id}")))
}

fn save(e: &mut Engine, tx: &mut Tx, list: Setlist) -> Result<Value, CommandError> {
    if list.name.is_empty() {
        return Err(CommandError::Rejected("a setlist needs a name".into()));
    }
    let issues: Vec<Value> = list
        .issues()
        .into_iter()
        .map(|i| json!({"field": i.field, "message": i.message}))
        .collect();
    let id = list.id;
    e.put_setlist(tx, list)?;
    Ok(json!({ "id": id.to_string(), "issues": issues }))
}

fn set_index(list: &Setlist, i: usize) -> Result<usize, CommandError> {
    if i < list.sets.len() {
        Ok(i)
    } else {
        Err(CommandError::InvalidParams(format!(
            "set {i} does not exist ({} sets)",
            list.sets.len()
        )))
    }
}

fn entry_index(list: &Setlist, set: usize, i: usize) -> Result<usize, CommandError> {
    let len = list.sets.get(set).map_or(0, |s| s.entries.len());
    if i < len {
        Ok(i)
    } else {
        Err(CommandError::InvalidParams(format!(
            "entry {i} does not exist in set {set} ({len} entries)"
        )))
    }
}

/// Insert at `set` (default: last) and `position` (default: end).
fn insert(list: &mut Setlist, p: &Params<'_>, entry: Entry) -> Result<(), CommandError> {
    let last = list.sets.len().saturating_sub(1);
    if list.sets.is_empty() {
        list.sets.push(Set {
            name: "Set 1".into(),
            entries: Vec::new(),
        });
    }
    let si = set_index(list, p.opt_usize("set")?.unwrap_or(last))?;
    let len = list.sets[si].entries.len();
    let pos = p.opt_usize("position")?.unwrap_or(len).min(len);
    list.sets[si].entries.insert(pos, entry);
    Ok(())
}

fn duration_of(e: &Engine) -> impl Fn(SongId) -> Option<u32> + '_ {
    |id| e.lib.song(id).and_then(|s| s.duration_secs)
}

fn summary(e: &Engine, s: &Setlist) -> Value {
    let t = s.timing(duration_of(e));
    json!({
        "id": s.id.to_string(),
        "name": s.name,
        "date": s.date,
        "songs": s.song_entries().count(),
        "total": format_duration(t.total),
    })
}

fn detail(e: &Engine, s: &Setlist) -> Value {
    let t = s.timing(duration_of(e));
    let sets: Vec<Value> = s
        .sets
        .iter()
        .zip(&t.per_set)
        .map(|(set, secs)| {
            let entries: Vec<Value> = set
                .entries
                .iter()
                .map(|entry| match entry {
                    Entry::Song(en) => {
                        let song = e.lib.song(en.song);
                        json!({
                            "type": "song",
                            "song": en.song.to_string(),
                            "missing": song.is_none(),
                            "title": song.map(|x| x.title.clone()),
                            "artist": song.map(|x| x.artist.clone()),
                            "key": en.key.clone().or_else(|| song.and_then(|x| x.key.clone())),
                            "tempo_bpm": en.tempo_bpm.or_else(|| song.and_then(|x| x.tempo_bpm)),
                            "duration_secs": song.and_then(|x| x.duration_secs),
                            "notes": en.notes,
                            "optional": en.optional,
                        })
                    }
                    Entry::Break { label, minutes } => {
                        json!({ "type": "break", "label": label, "minutes": minutes })
                    }
                })
                .collect();
            json!({ "name": set.name, "duration": format_duration(*secs), "entries": entries })
        })
        .collect();
    json!({
        "id": s.id.to_string(),
        "name": s.name,
        "date": s.date,
        "notes": s.notes,
        "sets": sets,
        "total": format_duration(t.total),
        "optional": format_duration(t.optional),
        "unknown_durations": t.unknown,
        "issues": s.issues().into_iter().map(|i| json!({"field": i.field, "message": i.message})).collect::<Vec<_>>(),
    })
}
