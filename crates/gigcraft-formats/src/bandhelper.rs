//! BandHelper's documented tab-delimited song layout.
//!
//! Columns in order: title, artist, tags, key, time signature, tempo,
//! duration, starting pitch, document file names, recording file names, MIDI
//! preset names, lyrics, chords, notes, MIDI song number, incoming program
//! change, then custom fields. `|` separates several values, a literal `\n`
//! stands for a line break. The header row is optional.
//!
//! Mapping: starting pitch becomes the custom field "Starting Pitch"; file
//! names, MIDI preset names, MIDI song number and incoming program change are
//! kept in the song's `extra["bandhelper"]` (GigCraft has no attachments or
//! named presets to put them in yet) so an export writes them back.

use crate::chordpro::{export_chordpro, split_header};
use crate::text::{
    check_size, format_tempo, lyrics_only, normalize_newlines, parse_duration, parse_tempo,
    split_list, text_to_chordpro,
};
use crate::{FormatError, ImportedSong, MAX_COLUMNS, MAX_RECORDS};
use gigcraft_model::{Song, format_duration};
use serde_json::{Map, Value, json};

/// Column names written in the header row of [`export_bandhelper_tsv`].
pub const BANDHELPER_COLUMNS: [&str; 16] = [
    "Title",
    "Artist",
    "Tags",
    "Key",
    "Time Signature",
    "Tempo",
    "Duration",
    "Starting Pitch",
    "Documents",
    "Recordings",
    "MIDI Presets",
    "Lyrics",
    "Chords",
    "Notes",
    "MIDI Song Number",
    "Incoming Program Change",
];

const STARTING_PITCH: &str = "Starting Pitch";
const EXTRA_KEY: &str = "bandhelper";
/// (column, key in `extra["bandhelper"]`, several values?)
const EXTRA_COLUMNS: [(usize, &str, bool); 5] = [
    (8, "documents", true),
    (9, "recordings", true),
    (10, "midi_presets", true),
    (14, "midi_song_number", false),
    (15, "incoming_program_change", false),
];

/// Import songs in BandHelper's tab-delimited layout. The chords column
/// (bracket chords or chords over lyrics) becomes the chart; without chords
/// the lyrics column does. Custom columns are named by the header row, or
/// "Column N" without one.
pub fn import_bandhelper_tsv(text: &str) -> Result<Vec<ImportedSong>, FormatError> {
    check_size(text.len())?;
    let text = normalize_newlines(text);
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        if rows.len() > MAX_RECORDS {
            return Err(FormatError::TooManyRecords { limit: MAX_RECORDS });
        }
        let cells = line
            .split('\t')
            .take(MAX_COLUMNS + 1)
            .map(cell_in)
            .collect();
        rows.push((n + 1, cells));
    }
    let header = rows
        .first()
        .filter(|(_, r)| is_header(r))
        .map(|(_, r)| r.clone());
    let mut songs = Vec::new();
    for (line_no, row) in rows.into_iter().skip(usize::from(header.is_some())) {
        songs.push(song_from_row(&row, line_no, header.as_deref()));
    }
    Ok(songs)
}

fn is_header(row: &[String]) -> bool {
    let n = |i: usize| {
        row.get(i)
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default()
    };
    matches!(
        n(0).as_str(),
        "title" | "song" | "name" | "song title" | "titel"
    ) && (row.len() == 1
        || matches!(n(1).as_str(), "artist" | "künstler" | "interpret" | "band")
        || n(3) == "key")
}

/// Unquote a cell written by a spreadsheet (`"a ""b"""`), then decode `\n`.
fn cell_in(raw: &str) -> String {
    let raw = raw.trim_end_matches('\r');
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .filter(|inner| !inner.replace("\"\"", "").contains('"'))
        .map(|inner| inner.replace("\"\"", "\""));
    unquoted.as_deref().unwrap_or(raw).replace("\\n", "\n")
}

fn cell_out(value: &str) -> String {
    let v = value
        .replace('\r', "")
        .replace('\t', " ")
        .replace('\n', "\\n");
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v
    }
}

fn song_from_row(row: &[String], line_no: usize, header: Option<&[String]>) -> ImportedSong {
    let mut warnings = Vec::new();
    let cell = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or_default();
    let title = cell(0);
    let mut song = Song::new(if title.is_empty() {
        warnings.push(format!("line {line_no} has no title"));
        format!("Untitled (line {line_no})")
    } else {
        title.to_string()
    });
    song.artist = cell(1).to_string();
    song.tags = split_list(cell(2), &['|']);
    song.key = Some(cell(3).to_string()).filter(|s| !s.is_empty());
    song.time_signature = Some(cell(4).to_string()).filter(|s| !s.is_empty());
    if !cell(5).is_empty() {
        song.tempo_bpm = parse_tempo(cell(5));
        if song.tempo_bpm.is_none() {
            warnings.push(format!(
                "tempo {:?} is not a number of BPM; left empty",
                cell(5)
            ));
        }
    }
    if !cell(6).is_empty() {
        song.duration_secs = parse_duration(cell(6));
        if song.duration_secs.is_none() {
            warnings.push(format!(
                "duration {:?} is not m:ss or seconds; left empty",
                cell(6)
            ));
        }
    }
    if !cell(7).is_empty() {
        song.custom.insert(STARTING_PITCH.into(), cell(7).into());
    }
    let mut extra = Map::new();
    for (col, key, many) in EXTRA_COLUMNS {
        let v = cell(col);
        if v.is_empty() {
            continue;
        }
        extra.insert(
            key.into(),
            if many {
                json!(split_list(v, &['|']))
            } else {
                json!(v)
            },
        );
    }
    if extra.contains_key("documents") || extra.contains_key("recordings") {
        warnings.push("document and recording files are not imported; their names are kept".into());
    }
    if extra.contains_key("midi_presets") {
        warnings.push("MIDI presets are named only; add their messages to the song's parts".into());
    }
    if !extra.is_empty() {
        song.extra.insert(EXTRA_KEY.into(), Value::Object(extra));
    }
    song.notes = cell(13).to_string();
    let last = row.len().min(MAX_COLUMNS);
    for col in 16..last {
        let v = cell(col);
        if v.is_empty() {
            continue;
        }
        let name = header
            .and_then(|h| h.get(col))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| format!("Column {}", col + 1));
        song.custom.insert(name, v.into());
    }
    if row.len() > MAX_COLUMNS {
        warnings.push(format!(
            "columns after the first {MAX_COLUMNS} were ignored"
        ));
    }
    let source = if cell(12).is_empty() {
        cell(11)
    } else {
        cell(12)
    };
    let body = text_to_chordpro(source);
    let chart = export_chordpro(&song, &body);
    ImportedSong {
        song,
        chart,
        warnings,
    }
}

/// Write songs and their charts in BandHelper's tab-delimited layout, with a
/// header row. The chords column holds the chart body (ChordPro, header
/// directives removed), the lyrics column the same without chords. Custom
/// fields of all songs become extra columns in order of first appearance.
#[must_use]
pub fn export_bandhelper_tsv(songs: &[(Song, String)]) -> String {
    let mut custom: Vec<&str> = Vec::new();
    for (song, _) in songs {
        for name in song.custom.keys() {
            if name != STARTING_PITCH && !custom.contains(&name.as_str()) {
                custom.push(name);
            }
        }
    }
    let mut out = String::new();
    let header: Vec<String> = BANDHELPER_COLUMNS
        .iter()
        .map(|s| (*s).to_string())
        .chain(custom.iter().map(|s| cell_out(s)))
        .collect();
    out.push_str(&header.join("\t"));
    out.push('\n');
    for (song, chart) in songs {
        let (_, body) = split_header(chart);
        let extra = song.extra.get(EXTRA_KEY).and_then(Value::as_object);
        let extra_cell = |key: &str| -> String {
            match extra.and_then(|e| e.get(key)) {
                Some(Value::Array(items)) => items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|s| s.replace('|', "/"))
                    .collect::<Vec<_>>()
                    .join("|"),
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                _ => String::new(),
            }
        };
        let mut cells: Vec<String> = vec![
            song.title.clone(),
            song.artist.clone(),
            song.tags
                .iter()
                .map(|t| t.replace('|', "/"))
                .collect::<Vec<_>>()
                .join("|"),
            song.key.clone().unwrap_or_default(),
            song.time_signature.clone().unwrap_or_default(),
            song.tempo_bpm.map(format_tempo).unwrap_or_default(),
            song.duration_secs.map(format_duration).unwrap_or_default(),
            song.custom.get(STARTING_PITCH).cloned().unwrap_or_default(),
            extra_cell("documents"),
            extra_cell("recordings"),
            extra_cell("midi_presets"),
            lyrics_only(&body),
            body.trim_end().to_string(),
            song.notes.clone(),
            extra_cell("midi_song_number"),
            extra_cell("incoming_program_change"),
        ];
        for name in &custom {
            cells.push(song.custom.get(*name).cloned().unwrap_or_default());
        }
        let cells: Vec<String> = cells.iter().map(|c| cell_out(c)).collect();
        out.push_str(&cells.join("\t"));
        out.push('\n');
    }
    out
}
