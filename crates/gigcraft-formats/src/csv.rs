//! Generic CSV: a small RFC 4180 reader and a column → field mapping.

use crate::chordpro::export_chordpro;
use crate::text::{
    check_size, normalize_newlines, parse_capo, parse_duration, parse_tempo, split_list,
    text_to_chordpro,
};
use crate::{FormatError, ImportedSong, MAX_RECORDS};
use gigcraft_model::{Link, Song};
use std::collections::BTreeMap;

/// Most columns read per row; further ones are dropped with a warning.
pub const MAX_COLUMNS: usize = 256;

/// What a CSV column holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvField {
    Title,
    Artist,
    Key,
    Tempo,
    TimeSignature,
    /// `m:ss`, `h:mm:ss` or seconds.
    Duration,
    Capo,
    /// Several tags separated by `,`, `;` or `|`.
    Tags,
    Notes,
    /// Lyrics, used as the chart when there is no chart column.
    Lyrics,
    /// ChordPro, bracket chords or chords over lyrics.
    Chart,
    /// One or more links separated by whitespace or `|`.
    Link,
    /// A custom field with this name.
    Custom(String),
    Ignore,
}

/// Which column holds what. Columns not listed are ignored.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CsvMapping {
    /// The first row names the columns and is not a song.
    pub has_header: bool,
    /// `None` detects `,`, `;` or tab from the first line.
    pub delimiter: Option<char>,
    /// Column index (from 0) → field.
    pub columns: BTreeMap<usize, CsvField>,
}

/// A parsed CSV file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvTable {
    pub delimiter: char,
    /// Rows with at least one non-empty cell.
    pub rows: Vec<Vec<String>>,
    pub warnings: Vec<String>,
}

/// Parse CSV text: quoted fields (with `""` for a quote and line breaks
/// inside), `,` `;` or tab as delimiter (detected from the first line when
/// `delimiter` is `None`), a UTF-8 BOM and any line endings. An unbalanced
/// quote takes the rest of the input into its field and adds a warning.
pub fn read_csv(text: &str, delimiter: Option<char>) -> Result<CsvTable, FormatError> {
    check_size(text.len())?;
    let text = normalize_newlines(text);
    let delim = delimiter.unwrap_or_else(|| detect_delimiter(&text));
    let mut table = CsvTable {
        delimiter: delim,
        rows: Vec::new(),
        warnings: Vec::new(),
    };
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let (mut quoted, mut in_quotes, mut dropped) = (false, false, false);
    let mut chars = text.chars().peekable();
    let end_row = |row: &mut Vec<String>, table: &mut CsvTable| -> Result<(), FormatError> {
        if row.iter().any(|c| !c.trim().is_empty()) {
            if table.rows.len() > MAX_RECORDS {
                return Err(FormatError::TooManyRecords { limit: MAX_RECORDS });
            }
            table.rows.push(std::mem::take(row));
        } else {
            row.clear();
        }
        Ok(())
    };
    let push_field = |row: &mut Vec<String>, field: &mut String, dropped: &mut bool| {
        if row.len() < MAX_COLUMNS {
            row.push(std::mem::take(field));
        } else {
            field.clear();
            *dropped = true;
        }
    };
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() && !quoted => {
                in_quotes = true;
                quoted = true;
            }
            '\n' => {
                push_field(&mut row, &mut field, &mut dropped);
                quoted = false;
                end_row(&mut row, &mut table)?;
            }
            c if c == delim => {
                push_field(&mut row, &mut field, &mut dropped);
                quoted = false;
            }
            c => field.push(c),
        }
    }
    if in_quotes {
        table
            .warnings
            .push("a quote is not closed; the rest of the file was read as one field".into());
    }
    if !field.is_empty() || !row.is_empty() {
        push_field(&mut row, &mut field, &mut dropped);
        end_row(&mut row, &mut table)?;
    }
    if dropped {
        table.warnings.push(format!(
            "columns after the first {MAX_COLUMNS} were ignored"
        ));
    }
    Ok(table)
}

/// The most frequent of tab, `;` and `,` in the first line (outside quotes).
fn detect_delimiter(text: &str) -> char {
    let mut counts = [0usize; 3];
    let mut in_quotes = false;
    for c in text.chars().take(1 << 20) {
        match c {
            '\n' if !in_quotes => break,
            '"' => in_quotes = !in_quotes,
            '\t' if !in_quotes => counts[0] += 1,
            ';' if !in_quotes => counts[1] += 1,
            ',' if !in_quotes => counts[2] += 1,
            _ => {}
        }
    }
    let best = (0..3).max_by_key(|i| (counts[*i], 3 - i)).unwrap_or(2);
    ['\t', ';', ','][best]
}

fn normalize_header(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// The field a column name stands for (English and German names).
fn field_for(name: &str) -> Option<CsvField> {
    let n = normalize_header(name);
    let is = |names: &[&str]| names.contains(&n.as_str());
    Some(
        if is(&[
            "title",
            "songtitle",
            "song",
            "name",
            "songname",
            "titel",
            "liedtitel",
            "lied",
            "stück",
            "stueck",
            "track",
            "tracktitle",
        ]) {
            CsvField::Title
        } else if is(&[
            "artist",
            "artists",
            "interpret",
            "interpreter",
            "künstler",
            "kuenstler",
            "band",
            "by",
            "author",
            "originalartist",
            "originalinterpret",
        ]) {
            CsvField::Artist
        } else if is(&["key", "tonart", "originalkey", "songkey"]) {
            CsvField::Key
        } else if is(&["tempo", "bpm", "geschwindigkeit"]) {
            CsvField::Tempo
        } else if is(&[
            "time",
            "timesignature",
            "timesig",
            "meter",
            "signature",
            "taktart",
            "takt",
        ]) {
            CsvField::TimeSignature
        } else if is(&[
            "duration",
            "length",
            "dauer",
            "länge",
            "laenge",
            "spielzeit",
            "laufzeit",
            "zeit",
        ]) {
            CsvField::Duration
        } else if is(&["capo", "kapo", "kapodaster"]) {
            CsvField::Capo
        } else if is(&[
            "tags",
            "tag",
            "keywords",
            "schlagwörter",
            "schlagworte",
            "stichwörter",
            "stichworte",
            "kategorie",
            "kategorien",
            "category",
            "categories",
            "genre",
        ]) {
            CsvField::Tags
        } else if is(&[
            "notes",
            "note",
            "notizen",
            "notiz",
            "bemerkung",
            "bemerkungen",
            "anmerkung",
            "anmerkungen",
            "kommentar",
            "comment",
            "comments",
            "hinweis",
            "hinweise",
        ]) {
            CsvField::Notes
        } else if is(&["lyrics", "lyric", "text", "songtext", "liedtext"]) {
            CsvField::Lyrics
        } else if is(&[
            "chords",
            "chordpro",
            "chart",
            "akkorde",
            "chordsheet",
            "leadsheet",
        ]) {
            CsvField::Chart
        } else if is(&[
            "link",
            "links",
            "url",
            "chordslink",
            "ultimateguitar",
            "website",
            "webseite",
        ]) {
            CsvField::Link
        } else {
            return None;
        },
    )
}

/// Guess a mapping from a header row. Known names (English and German) map to
/// song fields, other non-empty names to custom fields, empty ones are
/// ignored. `has_header` is true when a title column was recognised; if not,
/// the mapping only takes column 0 as the title.
#[must_use]
pub fn guess_mapping<S: AsRef<str>>(header: &[S]) -> CsvMapping {
    let mut columns = BTreeMap::new();
    for (i, name) in header.iter().enumerate().take(MAX_COLUMNS) {
        let name = name.as_ref().trim();
        let field = match field_for(name) {
            Some(f) if !columns.values().any(|v| *v == f) => f,
            _ if name.is_empty() => CsvField::Ignore,
            _ => CsvField::Custom(name.to_string()),
        };
        columns.insert(i, field);
    }
    if columns.values().any(|f| *f == CsvField::Title) {
        CsvMapping {
            has_header: true,
            delimiter: None,
            columns,
        }
    } else {
        CsvMapping {
            has_header: false,
            delimiter: None,
            columns: BTreeMap::from([(0, CsvField::Title)]),
        }
    }
}

/// Import songs from CSV with a column mapping. Rows without a title are
/// called "Untitled (row N)"; values that cannot be read are left empty and
/// reported in the song's warnings. File-level warnings go to the first song.
pub fn import_csv(text: &str, mapping: &CsvMapping) -> Result<Vec<ImportedSong>, FormatError> {
    if !mapping.columns.values().any(|f| *f == CsvField::Title) {
        return Err(FormatError::Invalid(
            "the mapping has no title column".into(),
        ));
    }
    let table = read_csv(text, mapping.delimiter)?;
    let skip = usize::from(mapping.has_header);
    let mut songs = Vec::new();
    for (n, row) in table.rows.iter().enumerate().skip(skip) {
        songs.push(song_from_row(row, n + 1, mapping));
    }
    if let Some(first) = songs.first_mut() {
        first.warnings.splice(0..0, table.warnings);
    }
    Ok(songs)
}

fn song_from_row(row: &[String], row_number: usize, mapping: &CsvMapping) -> ImportedSong {
    let mut warnings = Vec::new();
    let mut song = Song::new(String::new());
    let (mut lyrics, mut chart) = (String::new(), String::new());
    for (col, field) in &mapping.columns {
        let Some(value) = row.get(*col).map(|v| v.trim()).filter(|v| !v.is_empty()) else {
            continue;
        };
        match field {
            CsvField::Title => song.title = value.into(),
            CsvField::Artist => song.artist = value.into(),
            CsvField::Key => song.key = Some(value.into()),
            CsvField::Tempo => {
                song.tempo_bpm = parse_tempo(value);
                if song.tempo_bpm.is_none() {
                    warnings.push(format!(
                        "tempo {value:?} is not a number of BPM; left empty"
                    ));
                }
            }
            CsvField::TimeSignature => song.time_signature = Some(value.into()),
            CsvField::Duration => {
                song.duration_secs = parse_duration(value);
                if song.duration_secs.is_none() {
                    warnings.push(format!(
                        "duration {value:?} is not m:ss or seconds; left empty"
                    ));
                }
            }
            CsvField::Capo => {
                song.capo = parse_capo(value);
                if song.capo.is_none() && value != "0" {
                    warnings.push(format!(
                        "capo {value:?} is not a fret from 1 to 24; left empty"
                    ));
                }
            }
            CsvField::Tags => {
                for t in split_list(value, &[',', ';', '|']) {
                    if !song.tags.contains(&t) {
                        song.tags.push(t);
                    }
                }
            }
            CsvField::Notes => {
                if !song.notes.is_empty() {
                    song.notes.push('\n');
                }
                song.notes.push_str(value);
            }
            CsvField::Lyrics => lyrics = value.into(),
            CsvField::Chart => chart = value.into(),
            CsvField::Link => {
                for url in value.split(|c: char| c.is_whitespace() || c == '|') {
                    if url.contains("://") && !song.links.iter().any(|l| l.url == url) {
                        song.links.push(Link {
                            url: url.into(),
                            label: None,
                        });
                    }
                }
            }
            CsvField::Custom(name) => {
                song.custom.insert(name.clone(), value.into());
            }
            CsvField::Ignore => {}
        }
    }
    if song.title.is_empty() {
        song.title = format!("Untitled (row {row_number})");
        warnings.push(format!("row {row_number} has no title"));
    }
    let source = if chart.is_empty() { lyrics } else { chart };
    let body = text_to_chordpro(&source);
    let chart = export_chordpro(&song, &body);
    ImportedSong {
        song,
        chart,
        warnings,
    }
}
