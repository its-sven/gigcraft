//! A setlist as plain text, for printing or sending to the band.

use crate::text::format_tempo;
use gigcraft_model::{Entry, Setlist, Song, SongId, format_duration};
use std::fmt::Write;

/// The setlist as numbered text: name and date, notes, then each set with its
/// songs numbered through the whole list (key and tempo, overrides of this gig
/// first, duration, "optional"), the entry notes indented below, breaks, and
/// the running times. Songs that `lookup` does not find are shown as
/// "(missing song)".
///
/// ```text
/// Summer Fest – 2026-07-04
///
/// Set 1
///  1. Harbour Lights – The Demo Band  [G, 96 BPM, 4:05]
///     Intro drums only
///     — Break: Change (15 min) —
///
/// Total: 19:05
/// ```
#[must_use]
pub fn export_setlist_text<'a>(
    setlist: &Setlist,
    lookup: impl Fn(SongId) -> Option<&'a Song>,
) -> String {
    let mut out = String::new();
    let _ = write!(out, "{}", one_line(&setlist.name));
    if let Some(d) = &setlist.date {
        let _ = write!(out, " – {}", one_line(d));
    }
    out.push('\n');
    if !setlist.notes.trim().is_empty() {
        out.push('\n');
        for line in setlist.notes.trim().lines() {
            let _ = writeln!(out, "{}", line.trim_end());
        }
    }
    let mut number = 0usize;
    for set in &setlist.sets {
        out.push('\n');
        let _ = writeln!(out, "{}", one_line(&set.name));
        if set.entries.is_empty() {
            out.push_str("    (no songs)\n");
        }
        for entry in &set.entries {
            match entry {
                Entry::Song(e) => {
                    number += 1;
                    let song = lookup(e.song);
                    let name = song.map_or_else(
                        || "(missing song)".to_string(),
                        |s| one_line(&s.display_name()),
                    );
                    let mut facts = Vec::new();
                    if let Some(k) = e.key.as_ref().or(song.and_then(|s| s.key.as_ref())) {
                        facts.push(one_line(k));
                    }
                    if let Some(t) = e.tempo_bpm.or(song.and_then(|s| s.tempo_bpm)) {
                        facts.push(format!("{} BPM", format_tempo(t)));
                    }
                    if let Some(d) = song.and_then(|s| s.duration_secs) {
                        facts.push(format_duration(d));
                    }
                    if e.optional {
                        facts.push("optional".into());
                    }
                    let _ = write!(out, "{number:>2}. {name}");
                    if !facts.is_empty() {
                        let _ = write!(out, "  [{}]", facts.join(", "));
                    }
                    out.push('\n');
                    for line in e.notes.trim().lines() {
                        let _ = writeln!(out, "    {}", line.trim());
                    }
                }
                Entry::Break { label, minutes } => {
                    let label = one_line(label);
                    let label = if label.is_empty() {
                        "Break".into()
                    } else {
                        format!("Break: {label}")
                    };
                    let _ = writeln!(out, "    — {label} ({minutes} min) —");
                }
            }
        }
    }
    let timing = setlist.timing(|id| lookup(id).and_then(|s| s.duration_secs));
    out.push('\n');
    if setlist.sets.len() > 1 {
        for (set, secs) in setlist.sets.iter().zip(&timing.per_set) {
            let _ = writeln!(out, "{}: {}", one_line(&set.name), format_duration(*secs));
        }
    }
    let _ = write!(out, "Total: {}", format_duration(timing.total));
    if timing.optional > 0 {
        let _ = write!(out, " (+ {} optional)", format_duration(timing.optional));
    }
    if timing.unknown > 0 {
        let _ = write!(
            out,
            ", {} song{} without a duration",
            timing.unknown,
            if timing.unknown == 1 { "" } else { "s" }
        );
    }
    out.push('\n');
    out
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
