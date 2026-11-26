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
            // Anything the user picks: a chart file, a library, a web shortcut
            // (.webloc, .url) or a saved Ultimate Guitar page.
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
            import_any(e, tx, name, &bytes)
        }
        "import.text" => {
            // Pasted text: a song, or links (tabs or a shared playlist).
            let text = p.str("text")?;
            let name = p.opt_str("name")?.unwrap_or("pasted.txt");
            import_any(e, tx, name, text.as_bytes())
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
            add_tab_links(e, tx, &links)
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
            add_playlist(e, tx, &playlist, source)
        }
        "import.ultimate_guitar_details" => {
            // A song's tab page (loaded by the app): fill in title, artist,
            // key, capo and tuning. Only details, never the tab itself.
            let sid: SongId = p.id("id")?;
            let html = p.str("html")?;
            let details = fmt::ultimate_guitar::parse_tab_html(html).ok_or_else(|| {
                CommandError::InvalidParams("no song details on that page".into())
            })?;
            let song = e
                .lib
                .song(sid)
                .ok_or_else(|| CommandError::NotFound(format!("no song with id {sid}")))?
                .clone();
            let updated = with_details(song.clone(), &details);
            let changed = updated != song;
            if changed {
                e.put_song(tx, updated.clone(), None)?;
            }
            Ok(json!({
                "changed": changed,
                "title": updated.title,
                "artist": updated.artist,
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

/// Import whatever the bytes are: a web shortcut, a saved web page, text
/// with Ultimate Guitar links, or a chart or library file.
fn import_any(
    e: &mut Engine,
    tx: &mut Tx,
    name: &str,
    bytes: &[u8],
) -> Result<Value, CommandError> {
    let lower = name.to_ascii_lowercase();
    if let Ok(text) = std::str::from_utf8(bytes) {
        let shortcut = lower.ends_with(".webloc")
            || lower.ends_with(".url")
            || text.trim_start().starts_with("[InternetShortcut]")
            || (text.contains("<plist") && text.contains("<key>URL</key>"));
        if shortcut && let Some(link) = fmt::ultimate_guitar::link_in_shortcut(text) {
            return import_links(e, tx, &link);
        }
        let head = text.trim_start().get(..200).unwrap_or(text.trim_start());
        let html = lower.ends_with(".html")
            || lower.ends_with(".htm")
            || head.to_ascii_lowercase().starts_with("<!doctype html")
            || head.to_ascii_lowercase().starts_with("<html");
        if html {
            return import_page(e, tx, text);
        }
        if looks_like_links(text) {
            return import_links(e, tx, text);
        }
    }
    let import = fmt::import_bytes(name, bytes).map_err(|err| match err {
        fmt::FormatError::Unsupported => CommandError::Rejected(
            "GigCraft cannot read this file. It reads ChordPro, OnSong, OpenSong, plain text, \
             BandHelper and CSV files, Cuebook libraries, web links and saved Ultimate Guitar pages."
                .into(),
        ),
        other => CommandError::Rejected(other.to_string()),
    })?;
    apply_import(e, tx, import)
}

/// Text that is (mostly) links rather than a chart: a share message, a list
/// of tab links, a playlist link.
fn looks_like_links(text: &str) -> bool {
    let ug = !fmt::ultimate_guitar::links_in_text(text).is_empty()
        || fmt::ultimate_guitar::page_link_in_text(text).is_some();
    if !ug {
        return false;
    }
    let other_lines = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.contains("://"))
        .count();
    let chart = text.lines().any(|l| {
        let l = l.trim_start();
        l.starts_with('{') || (l.starts_with('[') && !l.contains("://"))
    });
    !chart && other_lines <= 3
}

/// Tab links → songs; a playlist link → the app loads the page
/// (`{"fetch_playlist": url}`), nothing changes yet.
fn import_links(e: &mut Engine, tx: &mut Tx, text: &str) -> Result<Value, CommandError> {
    let links = fmt::ultimate_guitar::links_in_text(text);
    if !links.is_empty() {
        return add_tab_links(e, tx, &links);
    }
    if let Some(url) = fmt::ultimate_guitar::page_link_in_text(text) {
        return Ok(json!({ "kind": "fetch_playlist", "url": url }));
    }
    Err(CommandError::Rejected(
        "GigCraft imports Ultimate Guitar tab and playlist links. Other links can be added to a song in its editor."
            .into(),
    ))
}

/// A web page saved from the browser: a playlist or a single tab.
fn import_page(e: &mut Engine, tx: &mut Tx, html: &str) -> Result<Value, CommandError> {
    let playlist = fmt::ultimate_guitar::parse_playlist_html(html);
    if !playlist.links.is_empty() {
        return add_playlist(e, tx, &playlist, "");
    }
    let details = fmt::ultimate_guitar::parse_tab_html(html).ok_or_else(|| {
        CommandError::Rejected(
            "This web page has no Ultimate Guitar playlist or tab. Copy its link and paste it instead."
                .into(),
        )
    })?;
    let link = details
        .url
        .as_deref()
        .and_then(fmt::ultimate_guitar::parse_link);
    let existing = link.as_ref().and_then(|l| song_with_link(e, &l.url));
    let song = match existing {
        Some(id) => e.lib.song(id).cloned(),
        None => None,
    };
    let created = song.is_none();
    let mut song = song.unwrap_or_else(|| {
        let mut s = Song::new(&details.title);
        if let Some(l) = &link {
            s.links.push(Link {
                url: l.url.clone(),
                label: Some("Ultimate Guitar".into()),
            });
        }
        s
    });
    song = with_details(song, &details);
    let id = song.id;
    e.put_song(tx, song, if created { Some("") } else { None })?;
    Ok(json!({
        "kind": "links",
        "created": if created { vec![id.to_string()] } else { vec![] },
        "existing": if created { vec![] } else { vec![id.to_string()] },
        "skipped_existing": usize::from(!created),
        "first_song": id.to_string(),
        "fetch": [],
    }))
}

/// The song that already has this tab link.
fn song_with_link(e: &Engine, url: &str) -> Option<SongId> {
    let key = fmt::ultimate_guitar::link_key(url);
    e.lib
        .songs()
        .iter()
        .find(|s| {
            s.links
                .iter()
                .any(|l| fmt::ultimate_guitar::link_key(&l.url) == key)
        })
        .map(|s| s.id)
}

/// A new song for a tab link: title and artist from the link, the link attached.
fn song_for_link(link: &fmt::ultimate_guitar::UgLink) -> Song {
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
    song
}

/// One song per tab link (existing ones are kept). `fetch` lists the new
/// songs' pages, for the app to load and fill in with
/// `import.ultimate_guitar_details`.
fn add_tab_links(
    e: &mut Engine,
    tx: &mut Tx,
    links: &[fmt::ultimate_guitar::UgLink],
) -> Result<Value, CommandError> {
    let mut created = Vec::new();
    let mut existing = Vec::new();
    let mut fetch = Vec::new();
    for link in links {
        if let Some(id) = song_with_link(e, &link.url) {
            existing.push(id.to_string());
            continue;
        }
        let song = song_for_link(link);
        created.push(song.id.to_string());
        fetch.push(json!({ "song": song.id.to_string(), "url": link.url }));
        e.put_song(tx, song, Some(""))?;
    }
    let first = created.first().or(existing.first()).cloned();
    Ok(json!({
        "kind": "links",
        "skipped_existing": existing.len(),
        "created": created,
        "existing": existing,
        "first_song": first,
        "fetch": fetch,
    }))
}

/// A playlist → a setlist in its order; songs already in the library are reused.
fn add_playlist(
    e: &mut Engine,
    tx: &mut Tx,
    playlist: &fmt::ultimate_guitar::UgPlaylist,
    source: &str,
) -> Result<Value, CommandError> {
    let mut ids = Vec::new();
    let mut created = 0;
    let mut existing = 0;
    let mut fetch = Vec::new();
    for link in &playlist.links {
        if let Some(id) = song_with_link(e, &link.url) {
            existing += 1;
            ids.push(id);
            continue;
        }
        let song = song_for_link(link);
        ids.push(song.id);
        fetch.push(json!({ "song": song.id.to_string(), "url": link.url }));
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
        "kind": "playlist",
        "setlist": sid.to_string(),
        "name": setlist_name,
        "songs": ids.len(),
        "created": created,
        "existing": existing,
        "fetch": fetch,
    }))
}

/// Take over what a tab page says: title and artist where they still come
/// from the link (or are empty), key, capo and tuning where none are set.
fn with_details(mut song: Song, d: &fmt::ultimate_guitar::TabDetails) -> Song {
    let from_link = song
        .links
        .iter()
        .find_map(|l| fmt::ultimate_guitar::parse_link(&l.url));
    let untouched = |current: &str, guessed: Option<&str>| {
        let c = current.trim();
        c.is_empty() || c == "Untitled" || guessed.is_some_and(|g| g.eq_ignore_ascii_case(c))
    };
    if !d.title.is_empty() && untouched(&song.title, from_link.as_ref().map(|l| l.title.as_str())) {
        song.title.clone_from(&d.title);
    }
    if !d.artist.is_empty()
        && untouched(&song.artist, from_link.as_ref().map(|l| l.artist.as_str()))
    {
        song.artist.clone_from(&d.artist);
    }
    if song.key.is_none() {
        song.key.clone_from(&d.key);
    }
    if song.capo.is_none() {
        song.capo = d.capo;
    }
    if let Some(tuning) = &d.tuning {
        let standard = tuning.to_ascii_lowercase().starts_with("standard")
            || tuning.split_whitespace().collect::<Vec<_>>() == ["E", "A", "D", "G", "B", "E"];
        if !standard {
            song.custom
                .entry("Tuning".into())
                .or_insert_with(|| tuning.clone());
        }
    }
    if !d.kind.is_empty() {
        for link in &mut song.links {
            if fmt::ultimate_guitar::is_ultimate_guitar(&link.url)
                && link
                    .label
                    .as_deref()
                    .is_none_or(|l| l.starts_with("Ultimate Guitar"))
            {
                link.label = Some(format!("Ultimate Guitar ({})", d.kind));
            }
        }
    }
    song
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
