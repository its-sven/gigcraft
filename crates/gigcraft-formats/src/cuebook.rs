//! Cuebook's library file (`Cuebook Library.json`).
//!
//! Cuebook (Apache-2.0, by the same author) keeps songs with Play Acoustic
//! presets per part, a chords link and notes, and setlists of song entries
//! with a note each. Ids are kept, so importing the same file again yields
//! the same song and setlist ids.
//!
//! Preset numbers: Cuebook stores a preset "as the pedal numbers it"
//! (parameter 0x2A4), counting from 1. Its simulated pedal, modelled on the
//! firmware, selects preset `n` with Program Change `n - 1`, so the wire
//! program is `number - 1`. (The app itself learns the offset from the
//! pedal's own Program Change messages at run time.)

use crate::chordpro::export_chordpro;
use crate::text::non_empty;
use crate::{FormatError, ImportedSong, MAX_INPUT_BYTES, MAX_RECORDS};
use gigcraft_model::{
    Entry, Link, MidiAction, MidiMessage, Part, Set, Setlist, SetlistId, Song, SongEntry, SongId,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

/// The device name used for Cuebook's presets.
pub const CUEBOOK_DEVICE: &str = "Play Acoustic";

/// Songs and setlists read from a Cuebook library.
#[derive(Debug, Clone, PartialEq)]
pub struct CuebookImport {
    pub songs: Vec<ImportedSong>,
    pub setlists: Vec<Setlist>,
    /// Problems with the file as a whole or with setlists.
    pub warnings: Vec<String>,
}

#[derive(Deserialize)]
struct CbLibrary {
    #[serde(default)]
    format: Option<i64>,
    #[serde(default)]
    songs: Vec<Value>,
    #[serde(default)]
    setlists: Vec<Value>,
}

#[derive(Deserialize)]
struct CbSong {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    key: String,
    #[serde(default)]
    tempo: Option<f64>,
    #[serde(default)]
    parts: Option<Vec<CbPart>>,
    /// Cuebook 0.1: one preset per song.
    #[serde(default)]
    preset: Option<CbPreset>,
    #[serde(default, rename = "chordsURL")]
    chords_url: Option<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Deserialize)]
struct CbPart {
    #[serde(default)]
    name: String,
    preset: CbPreset,
}

#[derive(Deserialize)]
struct CbPreset {
    number: i64,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Deserialize)]
struct CbSetlist {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    entries: Vec<Value>,
}

#[derive(Deserialize)]
struct CbEntry {
    #[serde(rename = "songID")]
    song_id: String,
    #[serde(default)]
    notes: String,
}

/// Import a Cuebook library file. Each preset becomes a part with a Program
/// Change on channel 1 for [`CUEBOOK_DEVICE`] (program = preset number − 1)
/// and the preset number and name in the part's notes; the chords link
/// becomes a link; each setlist becomes one set whose entries keep their
/// notes. Songs or entries that cannot be read are skipped with a warning.
pub fn import_cuebook(json: &[u8]) -> Result<CuebookImport, FormatError> {
    if json.len() > MAX_INPUT_BYTES {
        return Err(FormatError::TooLarge {
            size: json.len(),
            limit: MAX_INPUT_BYTES,
        });
    }
    if json.iter().all(u8::is_ascii_whitespace) {
        return Err(FormatError::Empty);
    }
    let lib: CbLibrary = serde_json::from_slice(json)
        .map_err(|e| FormatError::Invalid(format!("not a Cuebook library: {e}")))?;
    if lib.songs.len() > MAX_RECORDS || lib.setlists.len() > MAX_RECORDS {
        return Err(FormatError::TooManyRecords { limit: MAX_RECORDS });
    }
    let mut out = CuebookImport {
        songs: Vec::new(),
        setlists: Vec::new(),
        warnings: Vec::new(),
    };
    if lib.format.is_some_and(|f| f > 1) {
        out.warnings.push(format!(
            "the library was written in format {} by a newer Cuebook; unknown fields are ignored",
            lib.format.unwrap_or_default()
        ));
    }
    let mut ids: HashSet<SongId> = HashSet::new();
    for (i, value) in lib.songs.into_iter().enumerate() {
        match serde_json::from_value::<CbSong>(value) {
            Ok(cb) => {
                let imported = song(cb, &mut ids);
                out.songs.push(imported);
            }
            Err(e) => out.warnings.push(format!("song {} skipped: {e}", i + 1)),
        }
    }
    let mut setlist_ids: HashSet<SetlistId> = HashSet::new();
    for (i, value) in lib.setlists.into_iter().enumerate() {
        match serde_json::from_value::<CbSetlist>(value) {
            Ok(cb) => {
                let list = setlist(cb, &ids, &mut setlist_ids, &mut out.warnings);
                out.setlists.push(list);
            }
            Err(e) => out.warnings.push(format!("setlist {} skipped: {e}", i + 1)),
        }
    }
    Ok(out)
}

fn song(cb: CbSong, ids: &mut HashSet<SongId>) -> ImportedSong {
    let mut warnings = Vec::new();
    let title = non_empty(&cb.title).unwrap_or_else(|| "Untitled".into());
    let mut song = Song::new(title);
    match cb.id.parse::<SongId>() {
        Ok(id) if !ids.contains(&id) => song.id = id,
        Ok(_) => warnings.push(format!("id {} appears twice; a new id was given", cb.id)),
        Err(_) => warnings.push(format!("id {:?} is not a UUID; a new id was given", cb.id)),
    }
    ids.insert(song.id);
    song.artist = cb.artist.trim().to_string();
    song.key = non_empty(&cb.key);
    song.tempo_bpm = cb.tempo.filter(|t| t.is_finite() && *t > 0.0);
    song.notes = cb.notes;
    if let Some(url) = cb.chords_url.as_deref().and_then(non_empty) {
        song.links.push(Link {
            url,
            label: Some("Chords".into()),
        });
    }
    let parts = match (cb.parts, cb.preset) {
        (Some(parts), _) => parts,
        (None, Some(preset)) => vec![CbPart {
            name: String::new(),
            preset,
        }],
        (None, None) => Vec::new(),
    };
    for (i, p) in parts.into_iter().enumerate() {
        let number = p.preset.number;
        let preset_name = p.preset.name.as_deref().and_then(non_empty);
        let notes = match &preset_name {
            Some(n) => format!("Preset {number} · {n}"),
            None => format!("Preset {number}"),
        };
        let program = number
            .checked_sub(1)
            .and_then(|n| u8::try_from(n).ok())
            .filter(|p| *p <= 127);
        let actions = match program {
            Some(program) => vec![MidiAction {
                device: CUEBOOK_DEVICE.into(),
                message: MidiMessage::ProgramChange {
                    channel: 1,
                    program,
                    bank_msb: None,
                    bank_lsb: None,
                },
            }],
            None => {
                warnings.push(format!(
                    "preset {number} is outside 1–128; part {} has no MIDI message",
                    i + 1
                ));
                Vec::new()
            }
        };
        song.parts.push(Part {
            name: non_empty(&p.name).unwrap_or_else(|| format!("Part {}", i + 1)),
            actions,
            notes,
        });
    }
    let chart = export_chordpro(&song, "");
    ImportedSong {
        song,
        chart,
        warnings,
    }
}

fn setlist(
    cb: CbSetlist,
    songs: &HashSet<SongId>,
    ids: &mut HashSet<SetlistId>,
    warnings: &mut Vec<String>,
) -> Setlist {
    let mut list = Setlist::new(non_empty(&cb.name).unwrap_or_else(|| "Setlist".into()));
    match cb.id.parse::<SetlistId>() {
        Ok(id) if !ids.contains(&id) => list.id = id,
        _ => warnings.push(format!(
            "setlist {:?}: id {:?} is missing, repeated or not a UUID; a new id was given",
            list.name, cb.id
        )),
    }
    ids.insert(list.id);
    // Cuebook stores an instant (ISO 8601, UTC); its date part is kept.
    list.date = cb
        .date
        .as_deref()
        .and_then(|d| d.get(..10))
        .filter(|d| {
            let b = d.as_bytes();
            b.len() == 10
                && b[4] == b'-'
                && b[7] == b'-'
                && d.chars().filter(char::is_ascii_digit).count() == 8
        })
        .map(str::to_string);
    list.notes = cb.notes;
    let mut entries = Vec::new();
    let mut missing = 0usize;
    for value in cb.entries.into_iter().take(MAX_RECORDS) {
        let Ok(e) = serde_json::from_value::<CbEntry>(value) else {
            missing += 1;
            continue;
        };
        match e.song_id.parse::<SongId>() {
            Ok(id) if songs.contains(&id) => entries.push(Entry::Song(SongEntry {
                notes: e.notes,
                ..SongEntry::new(id)
            })),
            _ => missing += 1,
        }
    }
    if missing > 0 {
        warnings.push(format!(
            "setlist {:?}: {missing} entries skipped (song missing or unreadable)",
            list.name
        ));
    }
    list.sets = vec![Set {
        name: "Set 1".into(),
        entries,
    }];
    list
}
