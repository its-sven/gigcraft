use crate::{Entry, MidiMessage, Setlist, Song};

/// A problem found in a record. Records with issues still load; the UI shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Dotted path of the field, e.g. `parts[0].actions[1]`.
    pub field: String,
    pub message: String,
}

/// Records that can check themselves.
pub trait Validate {
    fn issues(&self) -> Vec<Issue>;
}

fn issue(field: impl Into<String>, message: impl Into<String>) -> Issue {
    Issue {
        field: field.into(),
        message: message.into(),
    }
}

fn check_tempo(field: &str, tempo: Option<f64>, out: &mut Vec<Issue>) {
    if let Some(t) = tempo
        && !(t.is_finite() && (20.0..=400.0).contains(&t))
    {
        out.push(issue(field, format!("tempo {t} is outside 20–400 BPM")));
    }
}

impl Validate for Song {
    fn issues(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        if self.title.trim().is_empty() {
            out.push(issue("title", "title is empty"));
        }
        check_tempo("tempo_bpm", self.tempo_bpm, &mut out);
        if let Some(c) = self.capo
            && c > 12
        {
            out.push(issue("capo", format!("capo {c} is above 12")));
        }
        for (pi, part) in self.parts.iter().enumerate() {
            if part.name.trim().is_empty() {
                out.push(issue(format!("parts[{pi}].name"), "part name is empty"));
            }
            for (ai, action) in part.actions.iter().enumerate() {
                let field = format!("parts[{pi}].actions[{ai}]");
                if action.device.trim().is_empty() {
                    out.push(issue(&field, "device is empty"));
                }
                if let Err(e) = action.message.to_bytes() {
                    out.push(issue(&field, e));
                }
                if let MidiMessage::SysEx { hex } = &action.message
                    && hex.len() > 4096
                {
                    out.push(issue(&field, "SysEx is longer than 2 KiB"));
                }
            }
        }
        out
    }
}

impl Validate for Setlist {
    fn issues(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        if self.name.trim().is_empty() {
            out.push(issue("name", "name is empty"));
        }
        if let Some(d) = &self.date
            && !is_iso_date(d)
        {
            out.push(issue("date", format!("date {d:?} is not YYYY-MM-DD")));
        }
        for (si, set) in self.sets.iter().enumerate() {
            for (ei, entry) in set.entries.iter().enumerate() {
                if let Entry::Song(e) = entry {
                    check_tempo(
                        &format!("sets[{si}].entries[{ei}].tempo_bpm"),
                        e.tempo_bpm,
                        &mut out,
                    );
                }
            }
        }
        out
    }
}

fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<u32>().ok());
    matches!((num(0..4), num(5..7), num(8..10)), (Some(_), Some(m), Some(d)) if (1..=12).contains(&m) && (1..=31).contains(&d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MidiAction, Part};

    #[test]
    fn song_issues() {
        let mut s = Song::new(" ");
        s.tempo_bpm = Some(1000.0);
        s.capo = Some(20);
        s.parts.push(Part {
            name: "Solo".into(),
            actions: vec![MidiAction {
                device: "Amp".into(),
                message: MidiMessage::ProgramChange {
                    channel: 17,
                    program: 1,
                    bank_msb: None,
                    bank_lsb: None,
                },
            }],
            notes: String::new(),
        });
        let fields: Vec<String> = s.issues().into_iter().map(|i| i.field).collect();
        assert_eq!(
            fields,
            ["title", "tempo_bpm", "capo", "parts[0].actions[0]"]
        );
        assert!(Song::new("Fine").issues().is_empty());
    }

    #[test]
    fn setlist_dates() {
        let mut l = Setlist::new("Gig");
        l.date = Some("2026-02-30".into());
        assert!(l.issues().is_empty(), "day range is loose on purpose");
        l.date = Some("31.12.2026".into());
        assert_eq!(l.issues().len(), 1);
    }
}
