//! Import and export of songs and setlists.
//!
//! Pure functions over strings and bytes: nothing here reads files or talks to
//! the network. Every importer returns its result as a value and never panics,
//! whatever the input; inputs are bounded ([`MAX_INPUT_BYTES`],
//! [`MAX_CHART_BYTES`], [`MAX_RECORDS`]).
//!
//! # Importers
//!
//! Each song importer yields [`ImportedSong`]: a [`Song`] record, its chart as
//! ChordPro text, and warnings about what could not be taken over.
//!
//! | Function | Input |
//! |---|---|
//! | [`detect`] | guesses the [`Format`] from a file name and its bytes |
//! | [`import_bytes`] | detects and imports any supported file |
//! | [`decode_text`] | bytes → text (UTF-8, UTF-16 with BOM, else Windows-1252) |
//! | [`import_chordpro`] | ChordPro; chart kept as is, the record filled from its directives |
//! | [`import_onsong`] | OnSong text: metadata lines, then sections and chords |
//! | [`import_opensong`] | OpenSong XML (`<song>` with `<lyrics>`) |
//! | [`import_plain_text`] | chords-over-lyrics or plain lyrics |
//! | [`import_bandhelper_tsv`] | BandHelper's documented tab-delimited song layout |
//! | [`read_csv`], [`guess_mapping`], [`import_csv`] | any CSV with a column mapping (EN/DE headers) |
//! | [`import_cuebook`] | Cuebook's library file (songs, presets, chords links, setlists) |
//! | [`ultimate_guitar`] | Ultimate Guitar tab links and shared playlist pages |
//!
//! # Exporters
//!
//! | Function | Output |
//! |---|---|
//! | [`export_chordpro`] | a chart whose header directives reflect the song record |
//! | [`export_bandhelper_tsv`] | BandHelper's tab-delimited song layout, with a header row |
//! | [`export_setlist_text`] | a numbered setlist for printing or sharing |
//!
//! Chords-over-lyrics are converted with this crate's own heuristics (chord
//! columns count characters, so umlauts stay aligned); ChordPro metadata is
//! read through `gigcraft-chordpro`.

mod bandhelper;
mod chordpro;
mod csv;
mod cuebook;
mod onsong;
mod opensong;
mod plain;
mod setlist_text;
mod text;
pub mod ultimate_guitar;

pub use bandhelper::{BANDHELPER_COLUMNS, export_bandhelper_tsv, import_bandhelper_tsv};
pub use chordpro::{export_chordpro, import_chordpro};
pub use csv::{CsvField, CsvMapping, CsvTable, MAX_COLUMNS, guess_mapping, import_csv, read_csv};
pub use cuebook::{CUEBOOK_DEVICE, CuebookImport, import_cuebook};
pub use onsong::import_onsong;
pub use opensong::import_opensong;
pub use plain::import_plain_text;
pub use setlist_text::export_setlist_text;
pub use text::decode_text;

use gigcraft_model::{Setlist, Song};

/// Largest input accepted by any importer (8 MiB).
pub const MAX_INPUT_BYTES: usize = 8 << 20;
/// Largest single chart accepted (the limit of `gigcraft-chordpro`, 1 MiB).
pub const MAX_CHART_BYTES: usize = gigcraft_chordpro::MAX_SOURCE_BYTES;
/// Most songs (rows) or setlists read from one input.
pub const MAX_RECORDS: usize = 10_000;

/// A song read from another format.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSong {
    pub song: Song,
    /// The chart as ChordPro text (may hold only header directives).
    pub chart: String,
    /// What could not be taken over, for the user to check.
    pub warnings: Vec<String>,
}

/// File formats this crate reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    ChordPro,
    OnSong,
    OpenSong,
    PlainText,
    BandHelperTsv,
    Csv,
    CuebookJson,
    Unknown,
}

/// Why an input could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("the input is {size} bytes; the limit is {limit} bytes")]
    TooLarge { size: usize, limit: usize },
    #[error("the input is empty")]
    Empty,
    #[error("the input is not text")]
    NotText,
    #[error("the input holds more than {limit} records")]
    TooManyRecords { limit: usize },
    #[error("unknown file format")]
    Unsupported,
    #[error("{0}")]
    Invalid(String),
}

/// What [`import_bytes`] found in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub format: Format,
    pub songs: Vec<ImportedSong>,
    pub setlists: Vec<Setlist>,
    /// Warnings about the file as a whole (per-song ones are in each song).
    pub warnings: Vec<String>,
}

/// Guess the format of a file from its name and content.
///
/// The extension decides where it is unambiguous (`.cho`, `.chordpro`,
/// `.chopro`, `.crd`, `.pro`, `.onsong`, `.tsv`, `.csv`); otherwise the start
/// of the content is examined.
#[must_use]
pub fn detect(file_name: &str, bytes: &[u8]) -> Format {
    let ext = file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "cho" | "chordpro" | "chopro" | "crd" | "pro" => return Format::ChordPro,
        "onsong" => return Format::OnSong,
        "tsv" => return Format::BandHelperTsv,
        "csv" => return Format::Csv,
        _ => {}
    }
    let head = &bytes[..bytes.len().min(64 * 1024)];
    let Ok(text) = decode_text(head) else {
        return Format::Unknown;
    };
    let text = text::normalize_newlines(&text);
    let t = text.trim_start();
    if t.is_empty() {
        return Format::Unknown;
    }
    // JSON starts with `{"`; a ChordPro directive with `{name`.
    let json_object = t
        .strip_prefix('{')
        .is_some_and(|r| r.trim_start().starts_with(['"', '}']));
    if ext == "json" || json_object {
        return if t.contains("\"songs\"") {
            Format::CuebookJson
        } else {
            Format::Unknown
        };
    }
    if ext == "xml" || t.starts_with('<') {
        return if t.contains("<song") {
            Format::OpenSong
        } else {
            Format::Unknown
        };
    }
    let lines: Vec<&str> = t
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let tabbed = lines
        .iter()
        .filter(|l| l.matches('\t').count() >= 2)
        .count();
    if !lines.is_empty() && tabbed * 5 >= lines.len() * 4 {
        return Format::BandHelperTsv;
    }
    if lines.iter().any(|l| text::directive(l).is_some()) {
        return Format::ChordPro;
    }
    if onsong::looks_like_onsong(t) {
        return Format::OnSong;
    }
    if let Some(first) = lines.first()
        && let Ok(table) = read_csv(first, None)
        && table.rows.first().is_some_and(|r| r.len() >= 2)
        && guess_mapping(&table.rows[0]).has_header
    {
        return Format::Csv;
    }
    Format::PlainText
}

/// Detect the format and import a file. CSV files need a header row that
/// [`guess_mapping`] understands; use [`import_csv`] with your own mapping otherwise.
pub fn import_bytes(file_name: &str, bytes: &[u8]) -> Result<Import, FormatError> {
    text::check_size(bytes.len())?;
    let format = detect(file_name, bytes);
    let hint = text::title_from_file_name(file_name);
    let mut out = Import {
        format,
        songs: Vec::new(),
        setlists: Vec::new(),
        warnings: Vec::new(),
    };
    match format {
        Format::CuebookJson => {
            let cb = import_cuebook(bytes)?;
            out.songs = cb.songs;
            out.setlists = cb.setlists;
            out.warnings = cb.warnings;
        }
        Format::Unknown => return Err(FormatError::Unsupported),
        _ => {
            let text = decode_text(bytes)?;
            out.songs = match format {
                Format::ChordPro => vec![chordpro::import(&text, &hint)?],
                Format::OnSong => vec![onsong::import(&text, &hint)?],
                Format::OpenSong => vec![import_opensong(&text)?],
                Format::PlainText => vec![import_plain_text(&text, &hint)?],
                Format::BandHelperTsv => import_bandhelper_tsv(&text)?,
                _ => {
                    let table = read_csv(&text, None)?;
                    let header = table.rows.first().cloned().unwrap_or_default();
                    let mapping = guess_mapping(&header);
                    if !mapping.has_header {
                        return Err(FormatError::Invalid(
                            "no known column names in the first row; map the columns by hand"
                                .into(),
                        ));
                    }
                    import_csv(&text, &mapping)?
                }
            };
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
