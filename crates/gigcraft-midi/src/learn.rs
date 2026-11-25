//! MIDI triggers for control inputs, and MIDI Learn.
//!
//! A [`Trigger`] says which incoming message fires an action (next song,
//! next part…). [`Trigger::from_event`] learns one from a message the user
//! sends; [`Trigger::matches`] checks incoming events. Triggers serialize to
//! TOML and JSON so mappings can be stored.

use crate::parser::Event;
use serde::{Deserialize, Serialize};
use std::fmt;

/// An inclusive range of 7-bit values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ValueRange {
    pub min: u8,
    pub max: u8,
}

impl ValueRange {
    #[must_use]
    pub fn contains(self, value: u8) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

/// An incoming message that fires an action.
///
/// `channel` is 1–16, or absent for any channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    /// A key pressed (Note On with velocity above 0).
    Note {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<u8>,
        note: u8,
    },
    /// A controller, optionally only within a value range (absent = any value).
    Cc {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<u8>,
        controller: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value_range: Option<ValueRange>,
    },
    ProgramChange {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<u8>,
        program: u8,
    },
    SongSelect {
        song: u8,
    },
}

impl Trigger {
    /// MIDI Learn: the trigger for a message the user just sent, on its
    /// channel. Releases (Note Off), clock and other messages give `None`.
    ///
    /// A learned CC keeps the half of the value range the message was in
    /// (64–127 or 0–63, the MIDI switch threshold), so a footswitch that
    /// sends 127 on press and 0 on release fires once per press.
    #[must_use]
    pub fn from_event(event: &Event) -> Option<Self> {
        Some(match *event {
            Event::NoteOn { channel, note, .. } => Self::Note {
                channel: Some(channel),
                note,
            },
            Event::ControlChange {
                channel,
                controller,
                value,
            } => Self::Cc {
                channel: Some(channel),
                controller,
                value_range: Some(if value >= 64 {
                    ValueRange { min: 64, max: 127 }
                } else {
                    ValueRange { min: 0, max: 63 }
                }),
            },
            Event::ProgramChange { channel, program } => Self::ProgramChange {
                channel: Some(channel),
                program,
            },
            Event::SongSelect(song) => Self::SongSelect { song },
            _ => return None,
        })
    }

    /// Whether `event` fires this trigger.
    #[must_use]
    pub fn matches(&self, event: &Event) -> bool {
        let ch = |want: Option<u8>, got: u8| want.is_none_or(|w| w == got);
        match (self, event) {
            (
                Self::Note { channel, note },
                Event::NoteOn {
                    channel: c,
                    note: n,
                    velocity,
                },
            ) => ch(*channel, *c) && note == n && *velocity > 0,
            (
                Self::Cc {
                    channel,
                    controller,
                    value_range,
                },
                Event::ControlChange {
                    channel: c,
                    controller: k,
                    value,
                },
            ) => {
                ch(*channel, *c)
                    && controller == k
                    && value_range.is_none_or(|r| r.contains(*value))
            }
            (
                Self::ProgramChange { channel, program },
                Event::ProgramChange {
                    channel: c,
                    program: p,
                },
            ) => ch(*channel, *c) && program == p,
            (Self::SongSelect { song }, Event::SongSelect(s)) => song == s,
            _ => false,
        }
    }

    /// The channel (1–16), or `None` for any channel / no channel.
    #[must_use]
    pub fn channel(&self) -> Option<u8> {
        match self {
            Self::Note { channel, .. }
            | Self::Cc { channel, .. }
            | Self::ProgramChange { channel, .. } => *channel,
            Self::SongSelect { .. } => None,
        }
    }

    /// The same trigger on any channel.
    #[must_use]
    pub fn with_any_channel(mut self) -> Self {
        match &mut self {
            Self::Note { channel, .. }
            | Self::Cc { channel, .. }
            | Self::ProgramChange { channel, .. } => *channel = None,
            Self::SongSelect { .. } => {}
        }
        self
    }

    /// Checks the numbers (e.g. of a trigger read from a file). An invalid
    /// trigger never matches anything, so this is for telling the user.
    pub fn validate(&self) -> Result<(), String> {
        let mut problems = Vec::new();
        if let Some(c) = self.channel()
            && !(1..=16).contains(&c)
        {
            problems.push(format!("channel {c} is outside 1–16"));
        }
        let mut data = |name: &str, v: u8| {
            if v > 127 {
                problems.push(format!("{name} {v} is outside 0–127"));
            }
        };
        match self {
            Self::Note { note, .. } => data("note", *note),
            Self::Cc {
                controller,
                value_range,
                ..
            } => {
                data("controller", *controller);
                if let Some(r) = value_range {
                    data("value", r.min);
                    data("value", r.max);
                    if r.min > r.max {
                        problems.push(format!("value range {}–{} is empty", r.min, r.max));
                    }
                }
            }
            Self::ProgramChange { program, .. } => data("program", *program),
            Self::SongSelect { song } => data("song", *song),
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("; "))
        }
    }
}

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let channel = |f: &mut fmt::Formatter<'_>, c: Option<u8>| match c {
            Some(c) => write!(f, ", ch {c}"),
            None => f.write_str(", any channel"),
        };
        match self {
            Self::Note { channel: c, note } => {
                write!(f, "Note {note}")?;
                channel(f, *c)
            }
            Self::Cc {
                channel: c,
                controller,
                value_range,
            } => {
                write!(f, "CC {controller}")?;
                if let Some(r) = value_range {
                    write!(f, " {}–{}", r.min, r.max)?;
                }
                channel(f, *c)
            }
            Self::ProgramChange {
                channel: c,
                program,
            } => {
                write!(f, "Program {program}")?;
                channel(f, *c)
            }
            Self::SongSelect { song } => write!(f, "Song select {song}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    fn note_on(channel: u8, note: u8, velocity: u8) -> Event {
        Event::NoteOn {
            channel,
            note,
            velocity,
        }
    }

    fn cc(channel: u8, controller: u8, value: u8) -> Event {
        Event::ControlChange {
            channel,
            controller,
            value,
        }
    }

    #[test]
    fn learn_and_match_a_note() {
        let t = Trigger::from_event(&note_on(2, 60, 90)).unwrap();
        assert_eq!(
            t,
            Trigger::Note {
                channel: Some(2),
                note: 60
            }
        );
        assert!(t.matches(&note_on(2, 60, 1)));
        assert!(!t.matches(&note_on(3, 60, 90)));
        assert!(!t.matches(&note_on(2, 61, 90)));
        assert!(!t.matches(&Event::NoteOff {
            channel: 2,
            note: 60,
            velocity: 0
        }));
        let any = t.with_any_channel();
        assert!(any.matches(&note_on(16, 60, 90)));
        assert_eq!(any.to_string(), "Note 60, any channel");
    }

    #[test]
    fn learned_cc_keeps_the_pressed_half() {
        let t = Trigger::from_event(&cc(1, 64, 127)).unwrap();
        assert!(t.matches(&cc(1, 64, 100)));
        assert!(!t.matches(&cc(1, 64, 0)), "the release does not fire");
        assert!(!t.matches(&cc(1, 65, 127)));
        assert_eq!(t.to_string(), "CC 64 64–127, ch 1");
        let low = Trigger::from_event(&cc(1, 64, 0)).unwrap();
        assert!(low.matches(&cc(1, 64, 10)) && !low.matches(&cc(1, 64, 64)));
        let any_value = Trigger::Cc {
            channel: None,
            controller: 1,
            value_range: None,
        };
        assert!(any_value.matches(&cc(9, 1, 0)));
    }

    #[test]
    fn learn_program_change_and_song_select() {
        let pc = Trigger::from_event(&Event::ProgramChange {
            channel: 1,
            program: 4,
        })
        .unwrap();
        assert!(pc.matches(&Event::ProgramChange {
            channel: 1,
            program: 4
        }));
        assert!(!pc.matches(&Event::ProgramChange {
            channel: 1,
            program: 5
        }));
        let song = Trigger::from_event(&Event::SongSelect(3)).unwrap();
        assert!(song.matches(&Event::SongSelect(3)));
        assert!(!song.matches(&Event::ProgramChange {
            channel: 1,
            program: 3
        }));
        for e in [
            Event::Clock,
            Event::NoteOff {
                channel: 1,
                note: 1,
                velocity: 0,
            },
            Event::SysEx(vec![0xF0, 0xF7]),
        ] {
            assert_eq!(Trigger::from_event(&e), None);
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Mappings {
        next_song: Vec<Trigger>,
    }

    #[test]
    fn triggers_are_stored_as_toml_and_json() {
        let m = Mappings {
            next_song: vec![
                Trigger::Note {
                    channel: None,
                    note: 60,
                },
                Trigger::Cc {
                    channel: Some(1),
                    controller: 64,
                    value_range: Some(ValueRange { min: 64, max: 127 }),
                },
                Trigger::ProgramChange {
                    channel: Some(16),
                    program: 0,
                },
                Trigger::SongSelect { song: 2 },
            ],
        };
        let toml_text = toml::to_string(&m).unwrap();
        assert!(toml_text.contains("type = \"cc\""), "{toml_text}");
        assert_eq!(toml::from_str::<Mappings>(&toml_text).unwrap(), m);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(serde_json::from_str::<Mappings>(&json).unwrap(), m);
        let hand_written: Mappings = toml::from_str(
            r#"
            [[next_song]]
            type = "note"
            note = 62
            "#,
        )
        .unwrap();
        assert!(hand_written.next_song[0].matches(&note_on(5, 62, 100)));
    }

    #[test]
    fn validation() {
        assert!(
            Trigger::Note {
                channel: Some(1),
                note: 60
            }
            .validate()
            .is_ok()
        );
        let bad = Trigger::Cc {
            channel: Some(17),
            controller: 200,
            value_range: Some(ValueRange { min: 100, max: 10 }),
        };
        let message = bad.validate().unwrap_err();
        assert!(message.contains("channel 17"), "{message}");
        assert!(message.contains("controller 200"), "{message}");
        assert!(message.contains("empty"), "{message}");
        assert!(!bad.matches(&cc(1, 72, 50)));
    }
}
