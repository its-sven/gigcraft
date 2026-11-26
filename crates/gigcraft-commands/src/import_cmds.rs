use crate::params::Params;
use crate::{CommandError, Engine, Tx};
use gigcraft_formats as fmt;
use gigcraft_model::{Entry, Link, Setlist, SetlistId, Song, SongEntry, SongId};
use serde_json::{Value, json};

/// Largest file read by `import.file` (the formats crate also checks).
const MAX_IMPORT_BYTES: u64 = fmt::MAX_INPUT_BYTES as u64;

pub(crate) fn run(
    e: &mut Engine,
    id: &str,
    p: &Params<'_>,
    tx: &mut Tx,
) -> Result<Value, CommandError> {
    match id {
        "import.file" => {
            let path = std::path::PathBuf::from(p.str("path")?);
            let meta = std::fs::metadata(&path)
                .map_err(|err| CommandError::Rejected(format!("{}: {err}", path.display())))?;
            if !meta.is_file() || meta.len() > MAX_IMPORT_BYTES {
                return Err(CommandError::Rejected(format!(
                    "{} is not a file of at most 8 MB",
                    path.display()
                )));
            }
            let bytes = std::fs::read(&path)
                .map_err(|err| CommandError::Rejected(format!("{}: {err}", path.display())))?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("import");
            let import = fmt::import_bytes(name, &bytes)
                .map_err(|err| CommandError::Rejected(err.to_string()))?;
            apply_import(e, tx, import)
        }
        "import.text" => {
            let text = p.str("text")?;
            let name = p.opt_str("name")?.unwrap_or("pasted.txt");
            let import = fmt::import_bytes(name, text.as_bytes())
                .map_err(|err| CommandError::Rejected(err.to_string()))?;
            apply_import(e, tx, import)
        }
        "import.ultimate_guitar" => {
            // One or more Ultimate Guitar links in the text: one song each (links only, no tab content).
            let text = p.str("links")?;
            let links = fmt::ultimate_guitar::links_in_text(text);
            if links.is_empty() {
                return Err(CommandError::InvalidParams(
                    "no Ultimate Guitar tab links found".into(),
                ));
            }
            let mut created = Vec::new();
            let mut skipped = 0;
            for link in links {
                let key = fmt::ultimate_guitar::link_key(&link.url);
                let exists = e.lib.songs().iter().any(|s| {
                    s.links
                        .iter()
                        .any(|l| fmt::ultimate_guitar::link_key(&l.url) == key)
                });
                if exists {
                    skipped += 1;
                    continue;
                }
                let mut song = Song::new(if link.title.is_empty() {
                    "Untitled"
                } else {
                    &link.title
                });
                song.artist = link.artist.clone();
                song.links.push(Link {
                    url: link.url.clone(),
                    label: Some("Ultimate Guitar".into()),
                });
                created.push(song.id.to_string());
                e.put_song(tx, song, Some(""))?;
            }
            Ok(json!({ "created": created, "skipped_existing": skipped }))
        }
        "import.ultimate_guitar_playlist" => {
            // A shared playlist page (loaded by the app): one setlist in the
            // playlist's order; songs already in the library (same link) are reused.
            let html = p.str("html")?;
            let source = p.opt_str("url")?.unwrap_or_default();
            let playlist = fmt::ultimate_guitar::parse_playlist_html(html);
            if playlist.links.is_empty() {
                return Err(CommandError::InvalidParams(
                    "no Ultimate Guitar tabs found on that page (is the playlist shared?)".into(),
                ));
            }
            let mut ids = Vec::new();
            let mut created = 0;
            let mut existing = 0;
            for link in &playlist.links {
                let key = fmt::ultimate_guitar::link_key(&link.url);
                let found = e
                    .lib
                    .songs()
                    .iter()
                    .find(|s| {
                        s.links
                            .iter()
                            .any(|l| fmt::ultimate_guitar::link_key(&l.url) == key)
                    })
                    .map(|s| s.id);
                if let Some(id) = found {
                    existing += 1;
                    ids.push(id);
                    continue;
                }
                let mut song = Song::new(if link.title.is_empty() {
                    "Untitled"
                } else {
                    &link.title
                });
                song.artist = link.artist.clone();
                let label = if link.kind.is_empty() {
                    "Ultimate Guitar".to_string()
                } else {
                    format!("Ultimate Guitar ({})", link.kind)
                };
                song.links.push(Link {
                    url: link.url.clone(),
                    label: Some(label),
                });
                ids.push(song.id);
                created += 1;
                e.put_song(tx, song, Some(""))?;
            }
            let name = if playlist.name.trim().is_empty() {
                "Ultimate Guitar playlist".to_string()
            } else {
                playlist.name.trim().to_string()
            };
            let mut setlist = Setlist::new(name);
            if !source.is_empty() {
                setlist.notes = format!("Imported from {source}");
            }
            if let Some(set) = setlist.sets.first_mut() {
                set.entries = ids
                    .iter()
                    .map(|id| Entry::Song(SongEntry::new(*id)))
                    .collect();
            }
            let sid = setlist.id;
            let setlist_name = setlist.name.clone();
            e.put_setlist(tx, setlist)?;
            Ok(json!({
                "setlist": sid.to_string(),
                "name": setlist_name,
                "songs": ids.len(),
                "created": created,
                "existing": existing,
            }))
        }
        "export.chordpro" => {
            let sid: SongId = p.id("id")?;
            let song = e
                .lib
                .song(sid)
                .ok_or_else(|| CommandError::NotFound(format!("no song with id {sid}")))?;
            Ok(json!({ "text": fmt::export_chordpro(song, e.lib.chart(sid).unwrap_or_default()) }))
        }
        "export.setlist_text" => {
            let sid: SetlistId = p.id("id")?;
            let list = e
                .lib
                .setlist(sid)
                .ok_or_else(|| CommandError::NotFound(format!("no setlist with id {sid}")))?;
            Ok(json!({ "text": fmt::export_setlist_text(list, |id| e.lib.song(id)) }))
        }
        "export.bandhelper_tsv" => {
            let pairs: Vec<(Song, String)> = e
                .lib
                .songs()
                .into_iter()
                .map(|s| (s.clone(), e.lib.chart(s.id).unwrap_or_default().to_string()))
                .collect();
            Ok(json!({ "text": fmt::export_bandhelper_tsv(&pairs), "songs": pairs.len() }))
        }
        _ => Err(CommandError::UnknownCommand(id.to_string())),
    }
}

/// Add imported songs and setlists. Songs whose id already exists (a second
/// Cuebook import) are updated; everything is one undo step.
fn apply_import(e: &mut Engine, tx: &mut Tx, import: fmt::Import) -> Result<Value, CommandError> {
    let mut created = 0;
    let mut updated = 0;
    let mut warnings = import.warnings.clone();
    let mut first = None;
    for imp in import.songs {
        for w in &imp.warnings {
            warnings.push(format!("{}: {w}", imp.song.title));
        }
        let exists = e.lib.song(imp.song.id).is_some();
        let mut song = imp.song;
        if let Some(old) = e.lib.song(song.id) {
            song.attachments = old.attachments.clone();
        }
        first.get_or_insert(song.id);
        e.put_song(tx, song, Some(&imp.chart))?;
        if exists { updated += 1 } else { created += 1 }
    }
    let mut setlists = 0;
    for list in import.setlists {
        e.put_setlist(tx, list)?;
        setlists += 1;
    }
    Ok(json!({
        "format": format!("{:?}", import.format),
        "songs_created": created,
        "songs_updated": updated,
        "setlists": setlists,
        "first_song": first.map(|id| id.to_string()),
        "warnings": warnings,
    }))
}
