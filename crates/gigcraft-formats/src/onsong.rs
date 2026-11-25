//! OnSong text: a block of metadata lines first, then the chart.
//!
//! The metadata block runs to the first blank line. Its lines are `Name: value`
//! pairs (Title, Artist, Key, Tempo, Time, Duration, Capo, Tags…); a first line
//! without a name is the title and a second one the artist. The body has
//! section headings (`Verse 1:`, `Chorus:`), chords over lyrics or in brackets.

use crate::chordpro::export_chordpro;
use crate::text::{
    has_bracket_chords, is_chord_line, non_empty, normalize_newlines, parse_capo, parse_duration,
    parse_tempo, section_label, split_list, text_to_chordpro,
};
use crate::{FormatError, ImportedSong, MAX_CHART_BYTES};
use gigcraft_model::Song;

/// Metadata names understood in the header block, lowercased.
const KNOWN: [&str; 30] = [
    "title",
    "artist",
    "author",
    "by",
    "key",
    "tempo",
    "bpm",
    "time",
    "time signature",
    "timesig",
    "signature",
    "duration",
    "length",
    "capo",
    "tags",
    "tag",
    "keywords",
    "topic",
    "topics",
    "category",
    "copyright",
    "ccli",
    "album",
    "year",
    "book",
    "flow",
    "number",
    "notes",
    "note",
    "subtitle",
];

/// Import an OnSong file. Section headings become ChordPro sections, chords
/// over lyrics become inline `[C]` chords, bracket chords are kept. Metadata
/// names without a song field (Copyright, CCLI, Album, Flow…) become custom fields.
pub fn import_onsong(text: &str) -> Result<ImportedSong, FormatError> {
    import(text, "")
}

/// `Name: value` with a known name.
fn meta_line(line: &str) -> Option<(String, String)> {
    let (name, value) = line.split_once(':')?;
    let name = name.trim();
    let lower = name.to_lowercase();
    KNOWN
        .contains(&lower.as_str())
        .then(|| (name.to_string(), value.trim().to_string()))
}

/// Whether text starts like an OnSong file (a known `Name: value` line in its first block).
pub(crate) fn looks_like_onsong(text: &str) -> bool {
    text.lines()
        .map(str::trim)
        .take_while(|l| !l.is_empty())
        .take(8)
        .any(|l| meta_line(l).is_some_and(|(_, v)| !v.is_empty()))
}

pub(crate) fn import(text: &str, title_hint: &str) -> Result<ImportedSong, FormatError> {
    if text.len() > MAX_CHART_BYTES {
        return Err(FormatError::TooLarge {
            size: text.len(),
            limit: MAX_CHART_BYTES,
        });
    }
    let text = normalize_newlines(text);
    if text.trim().is_empty() {
        return Err(FormatError::Empty);
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut i = lines.iter().take_while(|l| l.trim().is_empty()).count();
    let mut warnings = Vec::new();
    let mut song = Song::new(String::new());
    let (mut bare, mut named) = (0usize, 0usize);
    while let Some(line) = lines.get(i) {
        let t = line.trim();
        if t.is_empty() {
            i += 1;
            break;
        }
        if let Some((name, value)) = meta_line(t) {
            named += 1;
            i += 1;
            apply(&mut song, &name, &value, &mut warnings);
            continue;
        }
        let chart_line = section_label(t).is_some()
            || is_chord_line(t)
            || has_bracket_chords(t)
            || t.starts_with('{');
        if chart_line || named > 0 || bare >= 2 {
            break;
        }
        if bare == 0 && song.title.is_empty() {
            song.title = t.to_string();
        } else if bare == 1 && song.artist.is_empty() {
            song.artist = t.to_string();
        } else {
            break;
        }
        bare += 1;
        i += 1;
    }
    if song.title.trim().is_empty() {
        song.title = non_empty(title_hint).unwrap_or_else(|| {
            warnings.push("no title found; the song is called \"Untitled\"".into());
            "Untitled".into()
        });
    }
    let body = text_to_chordpro(&lines[i.min(lines.len())..].join("\n"));
    let chart = export_chordpro(&song, &body);
    Ok(ImportedSong {
        song,
        chart,
        warnings,
    })
}

fn apply(song: &mut Song, name: &str, value: &str, warnings: &mut Vec<String>) {
    if value.is_empty() {
        return;
    }
    match name.to_lowercase().as_str() {
        "title" => song.title = value.into(),
        "artist" => song.artist = value.into(),
        "author" | "by" => {
            if song.artist.is_empty() {
                song.artist = value.into();
            } else {
                song.custom.insert(name.into(), value.into());
            }
        }
        "subtitle" => {
            if song.artist.is_empty() {
                song.artist = value.into();
            }
        }
        "key" => song.key = Some(value.into()),
        "tempo" | "bpm" => {
            song.tempo_bpm = parse_tempo(value);
            if song.tempo_bpm.is_none() {
                warnings.push(format!(
                    "tempo {value:?} is not a number of BPM; left empty"
                ));
            }
        }
        "time" | "time signature" | "timesig" | "signature" => {
            song.time_signature = Some(value.into());
        }
        "duration" | "length" => {
            song.duration_secs = parse_duration(value);
            if song.duration_secs.is_none() {
                warnings.push(format!(
                    "duration {value:?} is not m:ss or seconds; left empty"
                ));
            }
        }
        "capo" => {
            song.capo = parse_capo(value);
            if song.capo.is_none() && value != "0" {
                warnings.push(format!(
                    "capo {value:?} is not a fret from 1 to 24; left empty"
                ));
            }
        }
        "tags" | "tag" | "keywords" | "topic" | "topics" | "category" => {
            for tag in split_list(value, &[',', ';']) {
                if !song.tags.contains(&tag) {
                    song.tags.push(tag);
                }
            }
        }
        "notes" | "note" => {
            if !song.notes.is_empty() {
                song.notes.push('\n');
            }
            song.notes.push_str(value);
        }
        _ => {
            song.custom.insert(name.into(), value.into());
        }
    }
}
