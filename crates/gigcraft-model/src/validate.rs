use crate::{AttachmentKind, Entry, MidiMessage, Setlist, Song};

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
        for (i, a) in self.attachments.iter().enumerate() {
            if !crate::Attachment::is_safe_name(&a.file) {
                out.push(issue(
                    format!("attachments[{i}].file"),
                    format!("{:?} is not a plain file name", a.file),
                ));
            }
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
        check_audio(self, &mut out);
        out
    }
}

fn check_gain(field: &str, db: f64, out: &mut Vec<Issue>) {
    if !(db.is_finite() && crate::GAIN_DB_RANGE.contains(&db)) {
        out.push(issue(field, format!("gain {db} dB is outside −96…24 dB")));
    }
}

fn check_bus(field: &str, bus: u32, out: &mut Vec<Issue>) {
    if bus > crate::MAX_BUS {
        out.push(issue(
            field,
            format!("bus {bus} is above {}", crate::MAX_BUS),
        ));
    }
}

fn check_audio(song: &Song, out: &mut Vec<Issue>) {
    for (i, t) in song.tracks.iter().enumerate() {
        let field = |f: &str| format!("tracks[{i}].{f}");
        match song.attachments.iter().find(|a| a.file == t.file) {
            None => out.push(issue(
                field("file"),
                format!("{:?} is not an attachment of this song", t.file),
            )),
            Some(a) if a.kind() != AttachmentKind::Audio => out.push(issue(
                field("file"),
                format!("{:?} is not an audio file", t.file),
            )),
            Some(_) => {}
        }
        check_gain(&field("gain_db"), t.gain_db, out);
        if !(t.pan.is_finite() && (-1.0..=1.0).contains(&t.pan)) {
            out.push(issue(
                field("pan"),
                format!("pan {} is outside −1…1", t.pan),
            ));
        }
        check_bus(&field("bus"), t.bus, out);
    }
    if let Some(c) = &song.click {
        check_gain("click.volume_db", c.volume_db, out);
        check_bus("click.bus", c.bus, out);
        if c.count_in_bars > crate::MAX_COUNT_IN_BARS {
            out.push(issue(
                "click.count_in_bars",
                format!(
                    "{} count-in bars is above {}",
                    c.count_in_bars,
                    crate::MAX_COUNT_IN_BARS
                ),
            ));
        }
        if (c.enabled || c.count_in_bars > 0) && song.tempo_bpm.is_none() {
            out.push(issue("click", "the click needs the song's tempo"));
        }
        if let Some(ts) = &song.time_signature
            && crate::beats_per_bar(ts).is_none()
        {
            out.push(issue(
                "time_signature",
                format!("the click cannot count {ts:?}; write it like 4/4 or 6/8"),
            ));
        }
    }
    for (i, m) in song.markers.iter().enumerate() {
        if m.name.trim().is_empty() {
            out.push(issue(format!("markers[{i}].name"), "marker name is empty"));
        } else if song.markers[..i].iter().any(|o| o.name == m.name) {
            out.push(issue(
                format!("markers[{i}].name"),
                format!("marker {:?} appears twice", m.name),
            ));
        }
        if !(m.at_secs.is_finite() && m.at_secs >= 0.0) {
            out.push(issue(
                format!("markers[{i}].at_secs"),
                format!("start {} is not a time ≥ 0", m.at_secs),
            ));
        }
        if let Some(end) = m.end_secs
            && !(end.is_finite() && end > m.at_secs)
        {
            out.push(issue(
                format!("markers[{i}].end_secs"),
                format!("end {end} is not after the start"),
            ));
        }
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
    fn audio_issues() {
        use crate::{Attachment, ClickSettings, Marker, Track};
        let mut s = Song::new("Backing");
        s.attachments.push(Attachment {
            file: "drums.wav".into(),
            label: None,
        });
        s.attachments.push(Attachment {
            file: "chart.pdf".into(),
            label: None,
        });
        s.tracks.push(Track::new("drums.wav"));
        assert!(s.issues().is_empty());
        let mut bad = Track::new("chart.pdf");
        bad.gain_db = 40.0;
        bad.pan = -2.0;
        bad.bus = 99;
        s.tracks.push(bad);
        s.tracks.push(Track::new("missing.wav"));
        s.click = Some(ClickSettings {
            count_in_bars: 2,
            ..ClickSettings::default()
        });
        s.time_signature = Some("4/x".into());
        s.markers.push(Marker {
            name: "Chorus".into(),
            at_secs: 10.0,
            end_secs: Some(5.0),
        });
        s.markers.push(Marker {
            name: "Chorus".into(),
            at_secs: -1.0,
            end_secs: None,
        });
        let fields: Vec<String> = s.issues().into_iter().map(|i| i.field).collect();
        assert_eq!(
            fields,
            [
                "tracks[1].file",
                "tracks[1].gain_db",
                "tracks[1].pan",
                "tracks[1].bus",
                "tracks[2].file",
                "click",
                "time_signature",
                "markers[0].end_secs",
                "markers[1].name",
                "markers[1].at_secs",
            ]
        );
        s.tempo_bpm = Some(120.0);
        assert!(!s.issues().iter().any(|i| i.field == "click"));
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
