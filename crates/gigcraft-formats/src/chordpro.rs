//! ChordPro import (record from directives) and export (directives from record).

use crate::text::{
    directive, directive_value, format_tempo, non_empty, normalize_newlines, parse_capo,
    parse_duration, parse_tempo,
};
use crate::{FormatError, ImportedSong, MAX_CHART_BYTES};
use gigcraft_model::{Song, format_duration};

/// Directives [`export_chordpro`] writes from the song record.
const MANAGED: [&str; 8] = [
    "title", "artist", "key", "tempo", "time", "capo", "duration", "tag",
];
/// Of those, the ones that describe the whole song (never repeated in the body).
const SONG_LEVEL: [&str; 5] = ["title", "artist", "capo", "duration", "tag"];
/// Directives that belong to the header block at the top of a chart.
const HEADER: [&str; 21] = [
    "title",
    "subtitle",
    "artist",
    "composer",
    "lyricist",
    "arranger",
    "album",
    "year",
    "key",
    "tempo",
    "time",
    "capo",
    "duration",
    "tag",
    "copyright",
    "sorttitle",
    "sortartist",
    "ccli",
    "instrument",
    "user",
    "meta",
];

/// Import a ChordPro chart. The chart is kept as is (line endings become
/// `\n`); title, artist (or the first subtitle), key, tempo, time, capo,
/// duration (`m:ss` or seconds) and tags are read from its directives.
pub fn import_chordpro(text: &str) -> Result<ImportedSong, FormatError> {
    import(text, "")
}

pub(crate) fn import(text: &str, title_hint: &str) -> Result<ImportedSong, FormatError> {
    if text.len() > MAX_CHART_BYTES {
        return Err(FormatError::TooLarge {
            size: text.len(),
            limit: MAX_CHART_BYTES,
        });
    }
    let chart = normalize_newlines(text);
    if chart.trim().is_empty() {
        return Err(FormatError::Empty);
    }
    let m = gigcraft_chordpro::metadata(&chart);
    let mut warnings = Vec::new();
    let title = m
        .title
        .clone()
        .or_else(|| non_empty(title_hint))
        .unwrap_or_else(|| {
            warnings.push("the chart has no {title}; the song is called \"Untitled\"".into());
            "Untitled".into()
        });
    let mut song = Song::new(title);
    song.artist = if m.artists.is_empty() {
        m.subtitles.first().cloned().unwrap_or_default()
    } else {
        m.artists.join(", ")
    };
    song.key = m.keys.first().cloned();
    song.time_signature = m.times.first().cloned();
    if let Some(t) = m.tempos.first() {
        song.tempo_bpm = parse_tempo(t);
        if song.tempo_bpm.is_none() {
            warnings.push(format!("tempo {t:?} is not a number of BPM; left empty"));
        }
    }
    if let Some(c) = &m.capo {
        song.capo = parse_capo(c);
        if song.capo.is_none() && c.trim() != "0" {
            warnings.push(format!("capo {c:?} is not a fret from 1 to 24; left empty"));
        }
    }
    if let Some(d) = &m.duration {
        song.duration_secs = parse_duration(d);
        if song.duration_secs.is_none() {
            warnings.push(format!("duration {d:?} is not m:ss or seconds; left empty"));
        }
    }
    for tag in m.tags {
        if !song.tags.contains(&tag) {
            song.tags.push(tag);
        }
    }
    Ok(ImportedSong {
        song,
        chart,
        warnings,
    })
}

/// Split a chart into its header directives that are *not* managed by the
/// song record (subtitle, album…, in order) and its body. Managed header
/// directives are dropped, and song-level ones (title, artist, capo,
/// duration, tag) are dropped from the body too; key, tempo and time stay in
/// the body, where they mark a change.
pub(crate) fn split_header(chart: &str) -> (Vec<String>, String) {
    let chart = normalize_newlines(chart);
    let lines: Vec<&str> = chart.lines().collect();
    let mut kept = Vec::new();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            if t.starts_with('#') {
                kept.push(t.to_string());
            }
            i += 1;
            continue;
        }
        match directive(t) {
            Some((name, _)) if MANAGED.contains(&name.as_str()) => i += 1,
            Some((name, _)) if HEADER.contains(&name.as_str()) => {
                kept.push(t.to_string());
                i += 1;
            }
            _ => break,
        }
    }
    let body: Vec<&str> = lines[i.min(lines.len())..]
        .iter()
        .filter(|l| !directive(l).is_some_and(|(n, _)| SONG_LEVEL.contains(&n.as_str())))
        .copied()
        .collect();
    let mut body = body.join("\n").trim_end().to_string();
    if !body.is_empty() {
        body.push('\n');
    }
    (kept, body)
}

/// A chart whose header directives reflect `song`: `{title}`, `{artist}`,
/// `{key}`, `{tempo}`, `{time}`, `{capo}`, `{duration}` and one `{tag}` per
/// tag come first, replacing the chart's own; other header directives
/// (subtitle, album…) follow, then the body unchanged. Key, tempo and time
/// changes later in the body are kept.
#[must_use]
pub fn export_chordpro(song: &Song, chart: &str) -> String {
    let (kept, body) = split_header(chart);
    let mut out = String::with_capacity(body.len() + 256);
    let mut put = |name: &str, value: &str| {
        let v = directive_value(value);
        if !v.is_empty() {
            out.push_str(&format!("{{{name}: {v}}}\n"));
        }
    };
    let title = if song.title.trim().is_empty() {
        "Untitled"
    } else {
        song.title.as_str()
    };
    put("title", title);
    put("artist", &song.artist);
    if let Some(k) = &song.key {
        put("key", k);
    }
    if let Some(t) = song.tempo_bpm.filter(|t| t.is_finite() && *t > 0.0) {
        put("tempo", &format_tempo(t));
    }
    if let Some(t) = &song.time_signature {
        put("time", t);
    }
    if let Some(c) = song.capo.filter(|c| *c > 0) {
        put("capo", &c.to_string());
    }
    if let Some(d) = song.duration_secs {
        put("duration", &format_duration(d));
    }
    for tag in &song.tags {
        put("tag", tag);
    }
    for line in kept {
        out.push_str(&line);
        out.push('\n');
    }
    if !body.trim().is_empty() {
        out.push('\n');
        out.push_str(body.trim_start_matches('\n'));
    }
    out
}
