//! Plain text charts: chords over lyrics, or lyrics alone.

use crate::chordpro::export_chordpro;
use crate::text::{Raw, classify, normalize_newlines, text_to_chordpro, title_from_file_name};
use crate::{FormatError, ImportedSong, MAX_CHART_BYTES};
use gigcraft_model::Song;

/// Import a plain-text chart. `title_hint` (a title or file name; a text
/// extension is dropped) names the song; when it is empty the first line is
/// the title, and a following `by …` line the artist. Chords over lyrics become
/// inline `[C]` chords, headings like `Chorus` or `[Verse 2]` become sections.
pub fn import_plain_text(text: &str, title_hint: &str) -> Result<ImportedSong, FormatError> {
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
    let mut warnings = Vec::new();
    let mut lines: Vec<&str> = text.lines().collect();
    let hint = title_from_file_name(title_hint);
    let mut song = Song::new(hint.clone());
    if hint.is_empty() {
        let first = lines.iter().position(|l| !l.trim().is_empty());
        if let Some(f) = first
            && matches!(classify(lines[f]), Raw::Lyric(_))
            && lines[f].trim().chars().count() <= 80
        {
            song.title = lines[f].trim().to_string();
            let mut cut = f + 1;
            if let Some(next) = lines.get(cut)
                && let Some(artist) = next
                    .trim()
                    .strip_prefix("by ")
                    .or_else(|| next.trim().strip_prefix("By "))
                    .or_else(|| next.trim().strip_prefix("von "))
            {
                song.artist = artist.trim().to_string();
                cut += 1;
            }
            lines.drain(..cut);
        } else {
            song.title = "Untitled".into();
            warnings.push("no title found; the song is called \"Untitled\"".into());
        }
    }
    let body = text_to_chordpro(&lines.join("\n"));
    let chart = export_chordpro(&song, &body);
    Ok(ImportedSong {
        song,
        chart,
        warnings,
    })
}
