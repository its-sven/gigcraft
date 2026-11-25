//! OpenSong XML: a `<song>` element with `<title>`, `<author>`, `<key>`,
//! `<tempo>`, `<time_sig>`, `<capo>`, `<theme>` and `<lyrics>`.
//!
//! Only the flat fields of the song are read (no XML tree is built). In the
//! lyrics, `[V1]` starts a section, a line starting with `.` holds chords for
//! the next line, ` ` starts a lyric line and `;` a comment.

use crate::chordpro::export_chordpro;
use crate::text::{
    Raw, directive_value, non_empty, normalize_newlines, parse_capo, parse_tempo, raws_to_chordpro,
    split_list,
};
use crate::{FormatError, ImportedSong, MAX_CHART_BYTES};
use gigcraft_model::Song;

/// Import an OpenSong song file. Lines numbered for several verses
/// (`1 text`) are read as plain lyric lines.
pub fn import_opensong(text: &str) -> Result<ImportedSong, FormatError> {
    if text.len() > MAX_CHART_BYTES {
        return Err(FormatError::TooLarge {
            size: text.len(),
            limit: MAX_CHART_BYTES,
        });
    }
    let xml = normalize_newlines(text);
    if xml.trim().is_empty() {
        return Err(FormatError::Empty);
    }
    let Some(start) = find_open_tag(&xml, "song", 0) else {
        return Err(FormatError::Invalid("no <song> element".into()));
    };
    let xml = &xml[start..];
    let field = |tag: &str| element_text(xml, tag).and_then(|s| non_empty(&s));
    let mut warnings = Vec::new();
    let mut song = Song::new(field("title").unwrap_or_else(|| {
        warnings.push("no <title>; the song is called \"Untitled\"".into());
        "Untitled".into()
    }));
    song.artist = field("author").unwrap_or_default();
    song.key = field("key");
    song.time_signature = field("time_sig");
    if let Some(t) = field("tempo") {
        song.tempo_bpm = parse_tempo(&t);
        if song.tempo_bpm.is_none() {
            song.custom.insert("Tempo".into(), t);
        }
    }
    if let Some(c) = field("capo") {
        song.capo = parse_capo(&c);
    }
    if let Some(theme) = field("theme") {
        song.tags = split_list(&theme, &[';', ',']);
    }
    for (tag, name) in [
        ("copyright", "Copyright"),
        ("ccli", "CCLI"),
        ("hymn_number", "Number"),
    ] {
        if let Some(v) = field(tag) {
            song.custom.insert(name.into(), v);
        }
    }
    let lyrics = element_text(xml, "lyrics").unwrap_or_default();
    let mut numbered = false;
    let raws: Vec<Raw> = lyrics
        .lines()
        .filter_map(|line| {
            let mut chars = line.chars();
            match chars.next() {
                None => Some(Raw::Blank),
                Some('.') => Some(Raw::Chords(format!(" {}", chars.as_str()))),
                Some(' ') => Some(if chars.as_str().trim().is_empty() {
                    Raw::Blank
                } else {
                    Raw::Lyric(format!(" {}", chars.as_str()).trim_end().to_string())
                }),
                Some(';') => Some(Raw::Keep(format!(
                    "{{comment: {}}}",
                    directive_value(chars.as_str())
                ))),
                Some('[') => Some(Raw::Label(section_name(
                    line.trim().trim_start_matches('[').trim_end_matches(']'),
                ))),
                Some(c) if c.is_ascii_digit() => {
                    numbered = true;
                    Some(Raw::Lyric(
                        format!(" {}", chars.as_str()).trim_end().to_string(),
                    ))
                }
                Some('-') if line.trim().chars().all(|c| c == '-') => None,
                Some(_) if line.trim() == "||" || line.trim() == "|" => None,
                Some(_) => Some(Raw::Lyric(line.trim_end().to_string())),
            }
        })
        .collect();
    if numbered {
        warnings.push("lines numbered for several verses were read as plain lines".into());
    }
    // Lyric lines keep the one-column prefix so chords stay aligned; drop it now.
    let body: String = raws_to_chordpro(&raws)
        .lines()
        .map(|l| l.strip_prefix(' ').unwrap_or(l))
        .collect::<Vec<_>>()
        .join("\n");
    let chart = export_chordpro(&song, &body);
    Ok(ImportedSong {
        song,
        chart,
        warnings,
    })
}

/// `V1` → "Verse 1", `C` → "Chorus", `B2` → "Bridge 2"; anything else as written.
fn section_name(code: &str) -> String {
    let code = code.trim();
    let mut chars = code.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    let rest = chars.as_str();
    let word = match first {
        Some('V') => "Verse",
        Some('C') => "Chorus",
        Some('B') => "Bridge",
        Some('P') => "Pre-Chorus",
        Some('T') => "Tag",
        Some('I') => "Intro",
        Some('O') => "Outro",
        _ => return code.to_string(),
    };
    if rest.is_empty() {
        word.to_string()
    } else if rest.chars().all(|c| c.is_ascii_digit()) {
        format!("{word} {rest}")
    } else {
        code.to_string()
    }
}

/// Byte offset of `<tag` (followed by `>`, `/` or whitespace) from `from`.
fn find_open_tag(xml: &str, tag: &str, from: usize) -> Option<usize> {
    let needle = format!("<{tag}");
    let mut pos = from;
    while let Some(rel) = xml.get(pos..)?.find(&needle) {
        let at = pos + rel;
        let after = xml[at + needle.len()..].chars().next();
        if matches!(after, Some('>' | '/') | Some(' ' | '\t' | '\n')) {
            return Some(at);
        }
        pos = at + needle.len();
    }
    None
}

/// The text of the first `<tag>…</tag>`, entities and CDATA decoded.
fn element_text(xml: &str, tag: &str) -> Option<String> {
    let open = find_open_tag(xml, tag, 0)?;
    let gt = open + xml[open..].find('>')?;
    if xml[..gt].ends_with('/') {
        return Some(String::new());
    }
    let close = format!("</{tag}>");
    let end = gt + 1 + xml[gt + 1..].find(&close)?;
    Some(decode_xml(&xml[gt + 1..end]))
}

fn decode_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("<![CDATA[") {
            let end = r.find("]]>").unwrap_or(r.len());
            out.push_str(&r[..end]);
            rest = r.get(end + 3..).unwrap_or_default();
            continue;
        }
        let Some(c) = rest.chars().next() else { break };
        if c == '&'
            && let Some((semi, _)) = rest.char_indices().take(12).find(|(_, c)| *c == ';')
            && let Some(ch) = entity(&rest[1..semi])
        {
            out.push(ch);
            rest = &rest[semi + 1..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub(crate) fn entity(name: &str) -> Option<char> {
    match name {
        "quot" => Some('"'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{A0}'),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}
