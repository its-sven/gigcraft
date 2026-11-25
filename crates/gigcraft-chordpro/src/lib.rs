//! ChordPro charts as a simple display model.
//!
//! Parsing is done by ChordSketch (`chordsketch-chordpro`, MIT). This crate
//! turns its AST into what a stage UI needs: blocks (verse, chorus…) of lines,
//! each line a list of chord/lyric segments. It keeps the ChordSketch API out
//! of the rest of GigCraft, so the parser can be replaced without touching UIs.
//!
//! Nothing here runs external programs: ChordSketch's ABC/Lilypond helpers are
//! never called, notation blocks are shown as plain text.

use chordsketch_chordpro as cs;
use chordsketch_chordpro::ast::{CommentStyle, DirectiveKind, Line};
use serde::Serialize;

/// Largest chart accepted (1 MiB). Larger input yields an empty chart and a warning.
pub const MAX_SOURCE_BYTES: usize = 1 << 20;

/// A chart ready to display.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct Chart {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub key: Option<String>,
    pub tempo: Option<String>,
    pub time: Option<String>,
    pub capo: Option<String>,
    pub duration: Option<String>,
    pub blocks: Vec<Block>,
}

/// A run of lines: a section (verse, chorus…) or the lines between sections.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Block {
    pub kind: BlockKind,
    /// Heading to show ("Chorus", "Verse: 2"), if any.
    pub label: Option<String>,
    pub lines: Vec<ChartLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Plain,
    Verse,
    Chorus,
    Bridge,
    Tab,
    Grid,
    /// `{chorus}`: repeat the chorus here.
    ChorusRecall,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChartLine {
    Lyrics { segments: Vec<Segment> },
    Comment { text: String, emphasis: bool },
    Empty,
}

/// A chord (shown above) and the lyric text that follows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Segment {
    pub chord: Option<String>,
    pub text: String,
}

/// The result of parsing: always a chart, plus warnings for what was skipped.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub chart: Chart,
    pub warnings: Vec<String>,
}

/// Parse ChordPro leniently, transposing chords by `semitones` (−11…11).
#[must_use]
pub fn parse(source: &str, semitones: i8) -> Parsed {
    if source.len() > MAX_SOURCE_BYTES {
        return Parsed {
            chart: Chart::default(),
            warnings: vec![format!(
                "chart is larger than {MAX_SOURCE_BYTES} bytes; not shown"
            )],
        };
    }
    let result = cs::parse_lenient(source);
    let warnings = result.errors.iter().map(ToString::to_string).collect();
    let song = if semitones % 12 == 0 {
        result.song
    } else {
        cs::transpose::transpose(&result.song, semitones.clamp(-11, 11))
    };
    Parsed {
        chart: to_chart(&song),
        warnings,
    }
}

fn kind_of(k: &DirectiveKind) -> BlockKind {
    match k {
        DirectiveKind::StartOfVerse => BlockKind::Verse,
        DirectiveKind::StartOfChorus => BlockKind::Chorus,
        DirectiveKind::StartOfBridge => BlockKind::Bridge,
        DirectiveKind::StartOfTab => BlockKind::Tab,
        DirectiveKind::StartOfGrid => BlockKind::Grid,
        _ => BlockKind::Other,
    }
}

fn to_chart(song: &cs::ast::Song) -> Chart {
    let m = &song.metadata;
    let mut chart = Chart {
        title: m.title.clone(),
        artist: (!m.artists.is_empty()).then(|| m.artists.join(", ")),
        key: m.key.clone(),
        tempo: m.tempo.clone(),
        time: m.time.clone(),
        capo: m.capo.clone(),
        duration: m.duration.clone(),
        blocks: Vec::new(),
    };
    let mut current = Block {
        kind: BlockKind::Plain,
        label: None,
        lines: Vec::new(),
    };
    let flush = |chart: &mut Chart, block: &mut Block, next: Block| {
        let done = std::mem::replace(block, next);
        if !done.lines.iter().all(|l| *l == ChartLine::Empty) || done.label.is_some() {
            chart.blocks.push(done);
        }
    };
    for line in &song.lines {
        match line {
            Line::Lyrics(l) => current.lines.push(ChartLine::Lyrics {
                segments: l
                    .segments
                    .iter()
                    .map(|s| Segment {
                        chord: s
                            .chord
                            .as_ref()
                            .map(|c| c.display.clone().unwrap_or_else(|| c.name.clone())),
                        text: s.text.clone(),
                    })
                    .collect(),
            }),
            Line::Comment(style, text) => current.lines.push(ChartLine::Comment {
                text: text.clone(),
                emphasis: !matches!(style, CommentStyle::Normal),
            }),
            Line::Empty => current.lines.push(ChartLine::Empty),
            Line::Directive(d) if d.is_section_start() => {
                let label = d.section_heading().map(|h| match h.value {
                    Some(v) if !v.is_empty() => format!("{}: {v}", h.label),
                    _ => h.label.into_owned(),
                });
                let next = Block {
                    kind: kind_of(&d.kind),
                    label,
                    lines: Vec::new(),
                };
                flush(&mut chart, &mut current, next);
            }
            Line::Directive(d) if d.is_section_end() => {
                let next = Block {
                    kind: BlockKind::Plain,
                    label: None,
                    lines: Vec::new(),
                };
                flush(&mut chart, &mut current, next);
            }
            Line::Directive(d) if d.kind == DirectiveKind::Chorus => {
                let label = Some(
                    d.value
                        .clone()
                        .filter(|v| !v.is_empty())
                        .unwrap_or_else(|| "Chorus".into()),
                );
                let recall = Block {
                    kind: BlockKind::ChorusRecall,
                    label,
                    lines: Vec::new(),
                };
                flush(&mut chart, &mut current, recall);
                let next = Block {
                    kind: BlockKind::Plain,
                    label: None,
                    lines: Vec::new(),
                };
                flush(&mut chart, &mut current, next);
            }
            Line::Directive(_) => {}
        }
    }
    flush(
        &mut chart,
        &mut current,
        Block {
            kind: BlockKind::Plain,
            label: None,
            lines: Vec::new(),
        },
    );
    chart
}

/// Pitch class (0–11) of a key or chord root such as "G", "F#m", "Bb", "Hm" (German H = B).
#[must_use]
pub fn pitch_class(key: &str) -> Option<i8> {
    let mut chars = key.trim().chars();
    let base: i8 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' | 'H' => 11,
        _ => return None,
    };
    let shift = match chars.next() {
        Some('#' | '♯') => 1,
        Some('b' | '♭') => -1,
        _ => 0,
    };
    Some((base + shift).rem_euclid(12))
}

/// Semitones (−5…6) to transpose a chart written in `from` so it sounds in `to`.
#[must_use]
pub fn semitones_between(from: &str, to: &str) -> Option<i8> {
    let d = (pitch_class(to)? - pitch_class(from)?).rem_euclid(12);
    Some(if d > 6 { d - 12 } else { d })
}

impl ChartLine {
    /// Chord line and lyric line as aligned monospace text (for plain views and tests).
    #[must_use]
    pub fn to_two_lines(&self) -> (String, String) {
        match self {
            ChartLine::Lyrics { segments } => {
                let (mut chords, mut lyrics) = (String::new(), String::new());
                for seg in segments {
                    let start = lyrics.chars().count().max(chords.chars().count());
                    pad(&mut chords, start);
                    pad(&mut lyrics, start);
                    if let Some(c) = &seg.chord {
                        chords.push_str(c);
                        chords.push(' ');
                    }
                    lyrics.push_str(&seg.text);
                }
                (chords.trim_end().to_string(), lyrics.trim_end().to_string())
            }
            ChartLine::Comment { text, .. } => (String::new(), text.clone()),
            ChartLine::Empty => (String::new(), String::new()),
        }
    }
}

fn pad(s: &mut String, width: usize) {
    let n = s.chars().count();
    if n < width {
        s.extend(std::iter::repeat_n(' ', width - n));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = "{title: Harbour Lights}\n{artist: The Demo Band}\n{key: G}\n{tempo: 96}\n\n\
{start_of_verse: 1}\n[G]Lanterns on the [C]water\n[D]boats are coming [G]home\n{end_of_verse}\n\n\
{start_of_chorus}\n[Em]Sing it [C]low\n{end_of_chorus}\n{comment: Solo over verse chords}\n{chorus}\n";

    #[test]
    fn parses_metadata_and_blocks() {
        let p = parse(DEMO, 0);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        let c = &p.chart;
        assert_eq!(c.title.as_deref(), Some("Harbour Lights"));
        assert_eq!(c.artist.as_deref(), Some("The Demo Band"));
        assert_eq!(c.key.as_deref(), Some("G"));
        let kinds: Vec<BlockKind> = c.blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                BlockKind::Verse,
                BlockKind::Chorus,
                BlockKind::Plain,
                BlockKind::ChorusRecall
            ]
        );
        assert_eq!(c.blocks[0].label.as_deref(), Some("Verse: 1"));
        let (chords, lyrics) = c.blocks[0].lines[0].to_two_lines();
        assert_eq!(chords, "G               C");
        assert_eq!(lyrics, "Lanterns on the water");
    }

    #[test]
    fn transposes_chords() {
        let p = parse(DEMO, 2);
        let ChartLine::Lyrics { segments } = &p.chart.blocks[0].lines[0] else {
            panic!("expected lyrics");
        };
        let chords: Vec<_> = segments.iter().filter_map(|s| s.chord.clone()).collect();
        assert_eq!(chords, ["A", "D"]);
    }

    #[test]
    fn key_distances() {
        assert_eq!(semitones_between("G", "A"), Some(2));
        assert_eq!(semitones_between("G", "F"), Some(-2));
        assert_eq!(semitones_between("Em", "Gm"), Some(3));
        assert_eq!(semitones_between("Bb", "H"), Some(1));
        assert_eq!(semitones_between("C", "F#"), Some(6));
        assert_eq!(semitones_between("C", "x"), None);
        assert_eq!(semitones_between("", "C"), None);
    }

    #[test]
    fn survives_garbage() {
        for input in [
            "",
            "{",
            "[[[",
            "{start_of_chorus}\n{end_of_verse}",
            "\u{0}\u{1}",
        ] {
            let _ = parse(input, 5);
        }
        let huge = "a".repeat(MAX_SOURCE_BYTES + 1);
        assert_eq!(parse(&huge, 0).warnings.len(), 1);
    }
}
