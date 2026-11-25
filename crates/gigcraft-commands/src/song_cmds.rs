use crate::params::Params;
use crate::{CommandError, Engine, Tx};
use gigcraft_model::{Song, SongId, Validate};
use serde_json::{Map, Value, json};

pub(crate) fn run(
    e: &mut Engine,
    id: &str,
    p: &Params<'_>,
    tx: &mut Tx,
) -> Result<Value, CommandError> {
    match id {
        "song.list" => {
            let query = p.opt_str("query")?.unwrap_or_default();
            Ok(Value::Array(
                e.lib.search(query).into_iter().map(summary).collect(),
            ))
        }
        "song.get" => {
            let sid: SongId = p.id("id")?;
            let song = find(e, sid)?;
            Ok(json!({
                "song": to_json(song)?,
                "chart": e.lib.chart(sid).unwrap_or_default(),
                "path": e.lib.song_path(sid).map(|pa| pa.display().to_string()),
                "attachments": song.attachments.iter().map(|a| json!({
                    "file": a.file,
                    "label": a.label,
                    "kind": a.kind(),
                    "path": e.lib.attachment_path(sid, &a.file).map(|pa| pa.display().to_string()),
                })).collect::<Vec<_>>(),
                "issues": issues(song),
            }))
        }
        "song.create" => {
            let title = p.str("title")?;
            let mut fields = p.raw_object()?;
            fields.remove("chart");
            let song = merge(&Song::new(title), &fields)?;
            check(&song)?;
            let chart = p.opt_str("chart")?;
            let out = json!({ "id": song.id.to_string(), "issues": issues(&song) });
            e.put_song(tx, song, Some(chart.unwrap_or_default()))?;
            Ok(out)
        }
        "song.update" => {
            let sid: SongId = p.id("id")?;
            let current = find(e, sid)?.clone();
            let fields = p.opt_object("fields")?.cloned().unwrap_or_default();
            let chart = p.opt_str("chart")?;
            if fields.is_empty() && chart.is_none() {
                return Err(CommandError::InvalidParams(
                    "give `fields` and/or `chart`".into(),
                ));
            }
            let song = merge(&current, &fields)?;
            check(&song)?;
            let out = json!({ "id": sid.to_string(), "issues": issues(&song) });
            e.put_song(tx, song, chart)?;
            Ok(out)
        }
        "song.trash" => {
            let sid: SongId = p.id("id")?;
            find(e, sid)?;
            e.remove_song(tx, sid)?;
            Ok(json!({ "id": sid.to_string() }))
        }
        "song.attach" => {
            let sid: SongId = p.id("id")?;
            let mut song = find(e, sid)?.clone();
            let path = std::path::PathBuf::from(p.str("path")?);
            let file = e.lib.copy_into_song(sid, &path)?;
            let label = p.opt_str("label")?.map(str::to_string);
            song.attachments.push(gigcraft_model::Attachment {
                file: file.clone(),
                label,
            });
            e.put_song(tx, song, None)?;
            Ok(json!({ "id": sid.to_string(), "file": file }))
        }
        "song.detach" => {
            let sid: SongId = p.id("id")?;
            let mut song = find(e, sid)?.clone();
            let file = p.str("file")?;
            let before = song.attachments.len();
            song.attachments.retain(|a| a.file != file);
            if song.attachments.len() == before {
                return Err(CommandError::NotFound(format!("no attachment {file:?}")));
            }
            e.put_song(tx, song, None)?;
            // The file goes to the trash too; undo restores the reference, the file stays in .trash/.
            e.lib.trash_attachment(sid, file)?;
            Ok(json!({ "id": sid.to_string(), "file": file }))
        }
        "song.chart" => {
            let sid: SongId = p.id("id")?;
            find(e, sid)?;
            let semis = p.opt_i64("transpose")?.unwrap_or(0);
            let semis = i8::try_from(semis.rem_euclid(12)).unwrap_or(0);
            let semis = if semis > 6 { semis - 12 } else { semis };
            let parsed = gigcraft_chordpro::parse(e.lib.chart(sid).unwrap_or_default(), semis);
            Ok(json!({
                "transpose": semis,
                "chart": serde_json::to_value(&parsed.chart).unwrap_or(Value::Null),
                "warnings": parsed.warnings,
            }))
        }
        _ => Err(CommandError::UnknownCommand(id.to_string())),
    }
}

fn find(e: &Engine, id: SongId) -> Result<&Song, CommandError> {
    e.lib
        .song(id)
        .ok_or_else(|| CommandError::NotFound(format!("no song with id {id}")))
}

pub(crate) fn summary(s: &Song) -> Value {
    json!({
        "id": s.id.to_string(),
        "title": s.title,
        "artist": s.artist,
        "key": s.key,
        "tempo_bpm": s.tempo_bpm,
        "duration_secs": s.duration_secs,
        "tags": s.tags,
    })
}

fn to_json(song: &Song) -> Result<Value, CommandError> {
    serde_json::to_value(song).map_err(|e| CommandError::Rejected(e.to_string()))
}

fn issues(song: &Song) -> Value {
    Value::Array(
        song.issues()
            .into_iter()
            .map(|i| json!({"field": i.field, "message": i.message}))
            .collect(),
    )
}

fn check(song: &Song) -> Result<(), CommandError> {
    if song.title.trim().is_empty() {
        return Err(CommandError::Rejected("a song needs a title".into()));
    }
    Ok(())
}

/// Apply partial fields to a song. `null` clears a field; `duration` accepts "m:ss".
fn merge(base: &Song, fields: &Map<String, Value>) -> Result<Song, CommandError> {
    let Value::Object(mut obj) = to_json(base)? else {
        return Err(CommandError::Rejected("song is not an object".into()));
    };
    for (k, v) in fields {
        match k.as_str() {
            "id" | "schema" => {
                return Err(CommandError::InvalidParams(format!(
                    "`{k}` cannot be changed"
                )));
            }
            "duration" => {
                let secs = match v {
                    Value::Null => None,
                    Value::String(s) => {
                        Some(gigcraft_model::parse_duration(s).ok_or_else(|| {
                            CommandError::InvalidParams("`duration` must be seconds or m:ss".into())
                        })?)
                    }
                    other => Some(
                        other
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .ok_or_else(|| {
                                CommandError::InvalidParams(
                                    "`duration` must be seconds or m:ss".into(),
                                )
                            })?,
                    ),
                };
                match secs {
                    Some(s) => obj.insert("duration_secs".into(), json!(s)),
                    None => obj.remove("duration_secs"),
                };
            }
            _ if v.is_null() => {
                obj.remove(k);
            }
            _ => {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let mut song: Song = serde_json::from_value(Value::Object(obj))
        .map_err(|err| CommandError::InvalidParams(format!("song fields: {err}")))?;
    song.id = base.id;
    song.schema = base.schema;
    Ok(song)
}

impl Params<'_> {
    /// All parameters as an object (for commands that take fields at top level).
    pub(crate) fn raw_object(&self) -> Result<Map<String, Value>, CommandError> {
        let mut m = Map::new();
        for key in [
            "title",
            "artist",
            "key",
            "tempo_bpm",
            "time_signature",
            "duration",
            "capo",
            "tags",
            "notes",
            "parts",
            "links",
            "custom",
            "tracks",
            "click",
            "markers",
            "chart",
        ] {
            if let Some(v) = self.raw(key) {
                m.insert(key.to_string(), v.clone());
            }
        }
        Ok(m)
    }
}
