//! Shared text helpers: decoding bytes, line endings, chord recognition and the
//! conversion of chords-over-lyrics text into ChordPro.

use crate::{FormatError, MAX_INPUT_BYTES};

/// Decode a file's bytes as text.
///
/// UTF-8 (with or without BOM) and UTF-16 with a BOM are read as such; bytes
/// that are not valid UTF-8 are read as Windows-1252 (what spreadsheet apps on
/// Windows write). Line endings are left alone. Binary data is refused.
pub fn decode_text(bytes: &[u8]) -> Result<String, FormatError> {
    check_size(bytes.len())?;
    let text = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        utf8_or_1252(rest)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        utf16(rest, u16::from_le_bytes)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        utf16(rest, u16::from_be_bytes)
    } else {
        utf8_or_1252(bytes)
    };
    if looks_binary(&text) {
        Err(FormatError::NotText)
    } else {
        Ok(text)
    }
}

pub(crate) fn check_size(len: usize) -> Result<(), FormatError> {
    if len > MAX_INPUT_BYTES {
        Err(FormatError::TooLarge {
            size: len,
            limit: MAX_INPUT_BYTES,
        })
    } else {
        Ok(())
    }
}

fn utf8_or_1252(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|b| cp1252(*b)).collect(),
    }
}

fn utf16(bytes: &[u8], read: fn([u8; 2]) -> u16) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|c| read(*c));
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Windows-1252; the five undefined bytes map to the C1 controls like Latin-1.
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[usize::from(b - 0x80)],
        _ => char::from(b),
    }
}

/// NUL bytes or many control characters mean this is not a text file.
fn looks_binary(text: &str) -> bool {
    let mut controls = 0usize;
    let mut total = 0usize;
    for c in text.chars().take(64 * 1024) {
        total += 1;
        if c == '\0' {
            return true;
        }
        if (c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{0C}'))
            || c == char::REPLACEMENT_CHARACTER
        {
            controls += 1;
        }
    }
    controls > 8 && controls * 20 > total
}

/// `\r\n` and `\r` become `\n`; a leading BOM is dropped.
pub(crate) fn normalize_newlines(text: &str) -> String {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text.to_string()
    }
}

/// "Songs/Harbour Lights.txt" → "Harbour Lights"; only known chart and data
/// extensions are dropped, so "Mr. Blue" stays as it is.
pub(crate) fn title_from_file_name(name: &str) -> String {
    const EXTENSIONS: [&str; 15] = [
        "txt", "text", "cho", "chordpro", "chopro", "crd", "pro", "onsong", "xml", "md", "tab",
        "tsv", "csv", "json", "opensong",
    ];
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = match base.rsplit_once('.') {
        Some((stem, ext))
            if !stem.trim().is_empty() && EXTENSIONS.contains(&ext.to_lowercase().as_str()) =>
        {
            stem
        }
        _ => base,
    };
    stem.trim().to_string()
}

/// Text for inside a `{directive: …}`: no braces or line breaks.
pub(crate) fn directive_value(s: &str) -> String {
    s.replace(['{', '}'], "")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

/// A `{name: value}` / `{name value}` / `{name}` line, name lowercased with
/// `t`/`st` resolved and `{meta: name value}` unwrapped.
pub(crate) fn directive(line: &str) -> Option<(String, String)> {
    let inner = line.trim().strip_prefix('{')?.strip_suffix('}')?;
    let inner = inner.trim();
    let split = inner.find(|c: char| c == ':' || c.is_whitespace());
    let (name, value) = match split {
        Some(i) => (&inner[..i], inner[i + 1..].trim()),
        None => (inner, ""),
    };
    let name = name.trim().to_lowercase();
    if name.is_empty() {
        return None;
    }
    if name == "meta" {
        let (n, v) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
        return Some((alias(&n.to_lowercase()), v.trim().to_string()));
    }
    Some((alias(&name), value.to_string()))
}

fn alias(name: &str) -> String {
    match name {
        "t" => "title".into(),
        "st" => "subtitle".into(),
        other => other.into(),
    }
}

// ---------------------------------------------------------------------------
// Song field values
// ---------------------------------------------------------------------------

/// "96", "96 BPM", "92,5" → BPM; nonsense or values outside 1–999 → None.
pub(crate) fn parse_tempo(s: &str) -> Option<f64> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(s.len());
    let n: f64 = s[..end].replace(',', ".").parse().ok()?;
    (n.is_finite() && (1.0..=999.0).contains(&n)).then_some(n)
}

/// "2", "2nd fret", "Capo 3" → fret; 0 and values above 24 → None.
pub(crate) fn parse_capo(s: &str) -> Option<u8> {
    let digits: String = s
        .trim()
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    let n: u8 = digits.parse().ok()?;
    (1..=24).contains(&n).then_some(n)
}

/// `m:ss`, `h:mm:ss` or seconds, with an optional unit word after it.
pub(crate) fn parse_duration(s: &str) -> Option<u32> {
    let s = s.trim();
    let end = s
        .find(|c: char| !(c.is_ascii_digit() || c == ':'))
        .unwrap_or(s.len());
    gigcraft_model::parse_duration(&s[..end])
}

/// Tempo as text: "96", or "92.5" when it has a fraction.
pub(crate) fn format_tempo(t: f64) -> String {
    if t.fract() == 0.0 && t.abs() < 1e9 {
        format!("{t:.0}")
    } else {
        format!("{t}")
    }
}

/// Split on any of `seps`, trimmed, without empties or duplicates.
pub(crate) fn split_list(s: &str, seps: &[char]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in s.split(seps) {
        let p = part.trim();
        if !p.is_empty() && !out.iter().any(|o| o == p) {
            out.push(p.to_string());
        }
    }
    out
}

pub(crate) fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

// ---------------------------------------------------------------------------
// Chords
// ---------------------------------------------------------------------------

/// A chord symbol such as `G`, `F#m7`, `Bbmaj7/D`, `Dsus4`, `E7(#9)`, `C6/9` or
/// the German `H7`. Deliberately strict, so lyric words are not taken for chords.
pub(crate) fn is_chord(token: &str) -> bool {
    if token.is_empty() || token.chars().count() > 16 {
        return false;
    }
    // "C6/9" is an extension, "D/F#" a bass note.
    let (body, bass) = match token.rsplit_once('/') {
        Some((_, "")) => return false,
        Some((_, bass)) if bass.chars().all(|c| c.is_ascii_digit()) => (token, None),
        Some((body, bass)) => (body, Some(bass)),
        None => (token, None),
    };
    if bass.is_some_and(|b| !is_note(b)) {
        return false;
    }
    let mut chars = body.chars();
    if !matches!(chars.next(), Some('A'..='H')) {
        return false;
    }
    let rest = chars.as_str();
    valid_suffix(rest.strip_prefix(['#', 'b', '♯', '♭']).unwrap_or(rest))
}

fn is_note(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some('A'..='H'))
        && match c.next() {
            None => true,
            Some('#' | 'b' | '♯' | '♭') => c.next().is_none(),
            Some(_) => false,
        }
}

fn valid_suffix(s: &str) -> bool {
    const WORDS: [&str; 14] = [
        "maj", "min", "dim", "aug", "sus", "add", "alt", "m", "M", "+", "-", "°", "ø", "Δ",
    ];
    let mut rest = s;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix(|c: char| c.is_ascii_digit() || c == '(' || c == ')') {
            rest = r;
            continue;
        }
        if let Some(r) = rest.strip_prefix(['#', 'b', '♯', '♭', '/'])
            && r.starts_with(|c: char| c.is_ascii_digit())
        {
            rest = r;
            continue;
        }
        match WORDS.iter().find(|w| rest.starts_with(**w)) {
            Some(w) => rest = &rest[w.len()..],
            None => return false,
        }
    }
    true
}

/// The chord in a chord-line token: `Am`, `(Am)` → `Am`, `N.C.` → `N.C.`.
fn chord_in(token: &str) -> Option<String> {
    if is_chord(token) {
        return Some(token.to_string());
    }
    if matches!(token, "N.C." | "N.C" | "NC" | "n.c.") {
        return Some("N.C.".into());
    }
    let inner = token.strip_prefix('(')?.strip_suffix(')')?;
    is_chord(inner).then(|| inner.to_string())
}

/// Bar lines, repeat marks and the like, allowed in a chord line.
fn is_neutral(token: &str) -> bool {
    if token.chars().all(|c| "|:/-%.*()~".contains(c)) {
        return true;
    }
    let t = token.trim_start_matches('(').trim_end_matches(')');
    let t = t.to_ascii_lowercase();
    let num = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
    t.strip_prefix('x').is_some_and(num) || t.strip_suffix('x').is_some_and(num)
}

fn tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start: Option<(usize, usize)> = None; // (char col, byte idx)
    for (col, (byte, c)) in line.char_indices().enumerate() {
        if c.is_whitespace() {
            if let Some((sc, sb)) = start.take() {
                out.push((sc, &line[sb..byte]));
            }
        } else if start.is_none() {
            start = Some((col, byte));
        }
    }
    if let Some((sc, sb)) = start {
        out.push((sc, &line[sb..]));
    }
    out
}

/// A line of chords only (with bar lines or repeat marks allowed).
pub(crate) fn is_chord_line(line: &str) -> bool {
    let toks = tokens(line);
    let mut chords = 0;
    for (_, t) in &toks {
        if chord_in(t).is_some() {
            chords += 1;
        } else if !is_neutral(t) {
            return false;
        }
    }
    chords > 0
}

/// Whether the line already holds ChordPro chords (`[G]`).
pub(crate) fn has_bracket_chords(line: &str) -> bool {
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        if chord_in(after[..close].trim()).is_some() {
            return true;
        }
        rest = &after[close + 1..];
    }
    false
}

fn expand_tabs(line: &str) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let mut out = String::with_capacity(line.len() + 16);
    let mut col = 0usize;
    for c in line.chars() {
        if c == '\t' {
            let next = (col / 8 + 1) * 8;
            out.extend(std::iter::repeat_n(' ', next - col));
            col = next;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Put the chords of `chord_line` into `lyric` as `[C]`, by character column.
pub(crate) fn merge_chords(chord_line: &str, lyric: &str) -> String {
    let chord_line = expand_tabs(chord_line);
    let lyric = expand_tabs(lyric);
    let mut out: Vec<char> = lyric.trim_end().chars().collect();
    let chords: Vec<(usize, String)> = tokens(&chord_line)
        .into_iter()
        .filter_map(|(col, t)| chord_in(t).map(|c| (col, c)))
        .collect();
    for (col, chord) in chords.iter().rev() {
        if out.len() < *col {
            out.resize(*col, ' ');
        }
        let ins: Vec<char> = format!("[{chord}]").chars().collect();
        out.splice(*col..*col, ins);
    }
    out.into_iter().collect::<String>().trim_end().to_string()
}

/// A chord line without lyrics: chords in brackets, bar lines kept.
pub(crate) fn bracket_chord_line(line: &str) -> String {
    let line = expand_tabs(line);
    let mut out = String::with_capacity(line.len() + 8);
    let mut last = 0usize;
    for (byte, tok) in byte_tokens(&line) {
        out.push_str(&line[last..byte]);
        match chord_in(tok) {
            Some(c) if tok.starts_with('(') => out.push_str(&format!("([{c}])")),
            Some(c) => out.push_str(&format!("[{c}]")),
            None => out.push_str(tok),
        }
        last = byte + tok.len();
    }
    out.trim_end().to_string()
}

fn byte_tokens(line: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (byte, c) in line.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((s, &line[s..byte]));
            }
        } else if start.is_none() {
            start = Some(byte);
        }
    }
    if let Some(s) = start {
        out.push((s, &line[s..]));
    }
    out
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

const SECTION_WORDS: [&str; 36] = [
    "verse",
    "chorus",
    "refrain",
    "bridge",
    "pre-chorus",
    "prechorus",
    "pre chorus",
    "post-chorus",
    "intro",
    "outro",
    "solo",
    "instrumental",
    "interlude",
    "tag",
    "coda",
    "ending",
    "break",
    "hook",
    "vamp",
    "riff",
    "turnaround",
    "breakdown",
    "middle",
    "strophe",
    "vers",
    "brücke",
    "zwischenspiel",
    "zwischenteil",
    "vorspiel",
    "nachspiel",
    "schluss",
    "schluß",
    "überleitung",
    "rap",
    "spoken",
    "ad lib",
];

/// A section heading such as `Verse 1:`, `Chorus`, `[Bridge]` or `(Chorus)`.
pub(crate) fn section_label(line: &str) -> Option<String> {
    let t = line.trim();
    if t.len() > 60 {
        return None;
    }
    for (open, close) in [('[', ']'), ('(', ')')] {
        if let Some(inner) = t.strip_prefix(open).and_then(|s| s.strip_suffix(close)) {
            let inner = inner.trim().trim_end_matches(':').trim();
            if inner.is_empty() || inner.contains(['[', ']', '(', ')', '{', '}']) {
                return None;
            }
            if chord_in(inner).is_some() || (open == '(' && !starts_with_section_word(inner)) {
                return None;
            }
            return Some(inner.to_string());
        }
    }
    let bare = t.strip_suffix(':').unwrap_or(t).trim();
    if bare.is_empty() || bare.contains(['[', ']', '{', '}', ':']) {
        return None;
    }
    starts_with_section_word(bare).then(|| bare.to_string())
}

/// `Verse`, `Verse 2`, `CHORUS x2`, `Pre-Chorus`, `Strophe 3`: a known word,
/// then at most a number or repeat mark.
fn starts_with_section_word(s: &str) -> bool {
    let lower = s.to_lowercase();
    SECTION_WORDS.iter().any(|w| {
        lower.strip_prefix(w).is_some_and(|rest| {
            let rest = rest.trim();
            rest.is_empty()
                || rest.split_whitespace().count() <= 2
                    && rest
                        .split_whitespace()
                        .all(|p| p.chars().all(|c| c.is_ascii_alphanumeric() || c == '.'))
                    && rest.chars().any(|c| c.is_ascii_digit())
        })
    })
}

/// ChordPro section for a heading: (`chorus`, None) for "Chorus",
/// (`verse`, Some("2")) for "Verse 2", (`intro`, None) for "Intro",
/// (`prechorus`, None) for "Pre-Chorus". Headings ChordPro has no section
/// for become a custom section named after their first word.
pub(crate) fn section_for(label: &str) -> (String, Option<String>) {
    const KNOWN: [(&str, &str); 7] = [
        ("chorus", "chorus"),
        ("chorus", "refrain"),
        ("verse", "verse"),
        ("verse", "strophe"),
        ("verse", "vers"),
        ("bridge", "bridge"),
        ("bridge", "brücke"),
    ];
    let label = label.trim();
    let lower = label.to_lowercase();
    let first_word = lower
        .split(|c: char| c.is_whitespace() || c.is_ascii_digit())
        .next()
        .unwrap_or_default();
    let known = KNOWN.iter().find(|(_, w)| {
        lower.strip_prefix(w).is_some_and(|r| {
            r.is_empty() || r.starts_with(|c: char| c.is_whitespace() || c.is_ascii_digit())
        })
    });
    let (kind, word_len) = match known {
        Some((k, w)) => ((*k).to_string(), w.len()),
        None if first_word.starts_with(|c: char| c.is_ascii_alphabetic())
            && first_word
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-') =>
        {
            // A `-` in a directive name starts a ChordPro selector, so drop it.
            (first_word.replace('-', ""), first_word.len())
        }
        None => return ("verse".into(), Some(label.to_string())),
    };
    let rest = label
        .get(word_len..)
        .or_else(|| lower.get(word_len..))
        .unwrap_or_default()
        .trim();
    let value = if rest.is_empty() {
        (lower.replace('-', "") != kind).then(|| label.to_string())
    } else if !rest.contains(char::is_whitespace) {
        Some(rest.to_string())
    } else {
        Some(label.to_string())
    };
    (kind, value)
}

// ---------------------------------------------------------------------------
// Body conversion
// ---------------------------------------------------------------------------

/// One classified source line.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Raw {
    Blank,
    Label(String),
    Chords(String),
    Lyric(String),
    /// Written as is (directives, comments, lines that already are ChordPro).
    Keep(String),
}

/// Classify a line of a plain-text or OnSong chart.
pub(crate) fn classify(line: &str) -> Raw {
    let t = line.trim();
    if t.is_empty() {
        Raw::Blank
    } else if t.starts_with('{') && t.ends_with('}') {
        Raw::Keep(t.to_string())
    } else if let Some(label) = section_label(line) {
        Raw::Label(label)
    } else if is_chord_line(line) {
        Raw::Chords(line.to_string())
    } else {
        Raw::Lyric(line.trim_end().to_string())
    }
}

/// Plain text (chords over lyrics, bracket chords, section headings) → ChordPro body.
pub(crate) fn text_to_chordpro(text: &str) -> String {
    let raws: Vec<Raw> = text.lines().map(classify).collect();
    raws_to_chordpro(&raws)
}

/// Turn classified lines into a ChordPro body. A section runs from its
/// heading to the next blank line or heading.
pub(crate) fn raws_to_chordpro(raws: &[Raw]) -> String {
    let mut out: Vec<String> = Vec::with_capacity(raws.len() + 8);
    let mut open: Option<String> = None;
    let close = |open: &mut Option<String>, out: &mut Vec<String>| {
        if let Some(kind) = open.take() {
            out.push(format!("{{end_of_{kind}}}"));
        }
    };
    let mut i = 0;
    while let Some(raw) = raws.get(i) {
        match raw {
            Raw::Blank => {
                close(&mut open, &mut out);
                out.push(String::new());
            }
            Raw::Label(label) => {
                close(&mut open, &mut out);
                let (kind, value) = section_for(label);
                let empty = matches!(
                    raws.get(i + 1),
                    None | Some(Raw::Blank | Raw::Label(_) | Raw::Keep(_))
                );
                if empty && kind == "chorus" {
                    out.push(match value {
                        Some(v) => format!("{{chorus: {}}}", directive_value(&v)),
                        None => "{chorus}".into(),
                    });
                } else if empty {
                    out.push(format!("{{comment: {}}}", directive_value(label)));
                } else {
                    out.push(match value {
                        Some(v) => format!("{{start_of_{kind}: {}}}", directive_value(&v)),
                        None => format!("{{start_of_{kind}}}"),
                    });
                    open = Some(kind);
                }
            }
            Raw::Chords(line) => {
                if let Some(Raw::Lyric(lyric)) = raws.get(i + 1)
                    && !has_bracket_chords(lyric)
                {
                    out.push(merge_chords(line, lyric));
                    i += 2;
                    continue;
                }
                out.push(bracket_chord_line(line));
            }
            Raw::Lyric(line) | Raw::Keep(line) => out.push(line.clone()),
        }
        i += 1;
    }
    close(&mut open, &mut out);
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    while out.first().is_some_and(String::is_empty) {
        out.remove(0);
    }
    let mut s = out.join("\n");
    if !s.is_empty() {
        s.push('\n');
    }
    s
}

/// The lyrics of a ChordPro body: chords and directives removed.
pub(crate) fn lyrics_only(chart: &str) -> String {
    let mut out = Vec::new();
    for line in chart.lines() {
        let t = line.trim();
        if t.starts_with('{') && t.ends_with('}') {
            if let Some((name, value)) = directive(t)
                && matches!(name.as_str(), "comment" | "c" | "comment_italic" | "ci")
            {
                out.push(value);
            }
            continue;
        }
        if t.starts_with('#') {
            continue;
        }
        let mut s = String::with_capacity(line.len());
        let mut rest = line;
        while let Some(open) = rest.find('[') {
            s.push_str(&rest[..open]);
            match rest[open..].find(']') {
                Some(close) => rest = &rest[open + close + 1..],
                None => {
                    rest = &rest[open..];
                    break;
                }
            }
        }
        s.push_str(rest);
        let s = s.trim_end();
        if !(s.trim().is_empty() && !line.trim().is_empty()) {
            out.push(s.to_string());
        }
    }
    let mut s = out.join("\n");
    s = s.trim_matches('\n').to_string();
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_are_strict() {
        for c in [
            "G", "Am", "F#m7", "Bbmaj7", "Dsus4", "Cadd9", "E7(#9)", "C6/9", "D/F#", "H7", "Bm7b5",
            "Gm/Bb", "C°", "A+", "Ebm",
        ] {
            assert!(is_chord(c), "{c}");
        }
        for w in [
            "", "Do", "Be", "Each", "Add", "Bad", "Dim", "Hello", "a", "am", "G/x", "Am/",
            "C#mMm7x",
        ] {
            assert!(!is_chord(w), "{w}");
        }
    }

    #[test]
    fn chord_lines() {
        assert!(is_chord_line("G    C   D"));
        assert!(is_chord_line("| G | C | x2"));
        assert!(is_chord_line("  (Am)   N.C."));
        assert!(!is_chord_line("Am Morgen"));
        assert!(!is_chord_line("A man walks"));
        assert!(!is_chord_line("| |"));
    }

    #[test]
    fn merges_by_character_column() {
        assert_eq!(
            merge_chords("G       C", "Lanterns on the water"),
            "[G]Lanterns[C] on the water"
        );
        // umlauts count as one column
        assert_eq!(merge_chords("   Am E", "Grüße aus"), "Grü[Am]ße [E]aus");
        assert_eq!(merge_chords("G     D", "Hi"), "[G]Hi    [D]");
        assert_eq!(
            bracket_chord_line("| G  | C (Am) |"),
            "| [G]  | [C] ([Am]) |"
        );
    }

    #[test]
    fn section_labels() {
        assert_eq!(section_label("Verse 1:").as_deref(), Some("Verse 1"));
        assert_eq!(section_label("[Chorus]").as_deref(), Some("Chorus"));
        assert_eq!(section_label("(Chorus)").as_deref(), Some("Chorus"));
        assert_eq!(section_label("PRE-CHORUS:").as_deref(), Some("PRE-CHORUS"));
        assert_eq!(section_label("Strophe 2").as_deref(), Some("Strophe 2"));
        assert_eq!(section_label("[Am]"), None);
        assert_eq!(section_label("(oh yeah)"), None);
        assert_eq!(section_label("And then she said:"), None);
        assert_eq!(section_label("Verse of the sea"), None);
        assert_eq!(section_for("Chorus"), ("chorus".into(), None));
        assert_eq!(section_for("Verse 2"), ("verse".into(), Some("2".into())));
        assert_eq!(
            section_for("Refrain"),
            ("chorus".into(), Some("Refrain".into()))
        );
        assert_eq!(section_for("Intro"), ("intro".into(), None));
        assert_eq!(section_for("Solo 2"), ("solo".into(), Some("2".into())));
        assert_eq!(section_for("Pre-Chorus"), ("prechorus".into(), None));
        assert_eq!(
            section_for("Überleitung"),
            ("verse".into(), Some("Überleitung".into()))
        );
    }

    #[test]
    fn decodes_encodings() {
        assert_eq!(decode_text(b"\xEF\xBB\xBFabc").ok().as_deref(), Some("abc"));
        assert_eq!(decode_text(b"Gr\xFC\xDFe").ok().as_deref(), Some("Grüße"));
        assert_eq!(decode_text(b"\x80 5").ok().as_deref(), Some("€ 5"));
        let utf16le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("Grüße".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(decode_text(&utf16le).ok().as_deref(), Some("Grüße"));
        assert_eq!(
            decode_text(b"PK\x03\x04\x00\x00"),
            Err(FormatError::NotText)
        );
        assert!(decode_text(&[0x01; 200]).is_err());
    }

    #[test]
    fn values() {
        assert_eq!(parse_tempo("96 BPM"), Some(96.0));
        assert_eq!(parse_tempo("92,5"), Some(92.5));
        assert_eq!(parse_tempo("fast"), None);
        assert_eq!(parse_tempo("0"), None);
        assert_eq!(parse_capo("Capo 3"), Some(3));
        assert_eq!(parse_capo("0"), None);
        assert_eq!(parse_capo("99"), None);
        assert_eq!(parse_duration("3:30 min"), Some(210));
        assert_eq!(parse_duration("210"), Some(210));
        assert_eq!(format_tempo(96.0), "96");
        assert_eq!(format_tempo(92.5), "92.5");
        assert_eq!(
            title_from_file_name("a/b\\Harbour Lights.txt"),
            "Harbour Lights"
        );
        assert_eq!(title_from_file_name("No extension"), "No extension");
        assert_eq!(title_from_file_name("Mr. Blue"), "Mr. Blue");
        assert_eq!(title_from_file_name("Song.CHO"), "Song");
        assert_eq!(
            directive("{meta: artist The Band}"),
            Some(("artist".into(), "The Band".into()))
        );
        assert_eq!(directive("{t:X}"), Some(("title".into(), "X".into())));
        assert_eq!(
            directive("{chorus}"),
            Some(("chorus".into(), String::new()))
        );
    }
}
