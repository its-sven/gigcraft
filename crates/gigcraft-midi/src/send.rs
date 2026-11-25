//! Turning a song part's MIDI actions into a timed send schedule.

use crate::parser::StreamParser;
use gigcraft_model::MidiAction;

/// When to send and how far apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScheduleOptions {
    /// Time of the first message (caller's clock, milliseconds).
    pub start_ms: u64,
    /// Gap between consecutive messages (0 = back to back). A bank select +
    /// Program Change action counts as several messages.
    pub gap_ms: u64,
}

/// One message to send at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduled {
    pub at_ms: u64,
    /// Device name from the action.
    pub device: String,
    /// One complete MIDI message.
    pub bytes: Vec<u8>,
    /// Index of the action this message comes from.
    pub action: usize,
}

/// An action that cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("action {} ({device}): {message}", .index + 1)]
pub struct ActionError {
    /// Index of the action in the part.
    pub index: usize,
    pub device: String,
    pub message: String,
}

/// The messages to send, in order, and the actions that were left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schedule {
    pub messages: Vec<Scheduled>,
    pub errors: Vec<ActionError>,
}

impl Schedule {
    /// Whether every action made it into the schedule.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Schedules `actions` in order. An invalid action is reported in
/// [`Schedule::errors`] and skipped; the others are still sent and the gaps
/// close up behind it.
#[must_use]
pub fn schedule(actions: &[MidiAction], options: ScheduleOptions) -> Schedule {
    let mut out = Schedule::default();
    let mut at = options.start_ms;
    let mut first = true;
    for (index, action) in actions.iter().enumerate() {
        let error = |message: String| ActionError {
            index,
            device: action.device.clone(),
            message,
        };
        if action.device.trim().is_empty() {
            out.errors.push(error("no device is set".into()));
            continue;
        }
        let bytes = match action.message.to_bytes() {
            Ok(b) => b,
            Err(message) => {
                out.errors.push(error(message));
                continue;
            }
        };
        let messages = StreamParser::new().messages(&bytes);
        if messages.is_empty() {
            out.errors.push(error("the message is empty".into()));
            continue;
        }
        for bytes in messages {
            if !first {
                at = at.saturating_add(options.gap_ms);
            }
            first = false;
            out.messages.push(Scheduled {
                at_ms: at,
                device: action.device.clone(),
                bytes,
                action: index,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gigcraft_model::MidiMessage;

    fn action(device: &str, message: MidiMessage) -> MidiAction {
        MidiAction {
            device: device.into(),
            message,
        }
    }

    #[test]
    fn messages_are_spaced_by_the_gap() {
        let actions = [
            action(
                "Helix",
                MidiMessage::ProgramChange {
                    channel: 1,
                    program: 5,
                    bank_msb: Some(0),
                    bank_lsb: None,
                },
            ),
            action(
                "Synth",
                MidiMessage::ControlChange {
                    channel: 2,
                    controller: 7,
                    value: 100,
                },
            ),
            action(
                "Synth",
                MidiMessage::SysEx {
                    hex: "F0 7D 01 F7".into(),
                },
            ),
        ];
        let s = schedule(
            &actions,
            ScheduleOptions {
                start_ms: 1000,
                gap_ms: 20,
            },
        );
        assert!(s.is_complete());
        let got: Vec<(u64, &str, &[u8], usize)> = s
            .messages
            .iter()
            .map(|m| (m.at_ms, m.device.as_str(), m.bytes.as_slice(), m.action))
            .collect();
        assert_eq!(
            got,
            vec![
                (1000, "Helix", &[0xB0, 0, 0][..], 0),
                (1020, "Helix", &[0xC0, 5][..], 0),
                (1040, "Synth", &[0xB1, 7, 100][..], 1),
                (1060, "Synth", &[0xF0, 0x7D, 0x01, 0xF7][..], 2),
            ]
        );
    }

    #[test]
    fn bad_actions_are_reported_and_the_rest_is_sent() {
        let actions = [
            action(
                "Helix",
                MidiMessage::ControlChange {
                    channel: 0,
                    controller: 7,
                    value: 1,
                },
            ),
            action("", MidiMessage::SongSelect { song: 3 }),
            action("Helix", MidiMessage::SongSelect { song: 3 }),
            action(
                "Synth",
                MidiMessage::SysEx {
                    hex: "F0 ZZ".into(),
                },
            ),
            action(
                "Helix",
                MidiMessage::NoteOn {
                    channel: 1,
                    note: 60,
                    velocity: 1,
                },
            ),
        ];
        let s = schedule(
            &actions,
            ScheduleOptions {
                start_ms: 0,
                gap_ms: 5,
            },
        );
        assert_eq!(
            s.errors.iter().map(|e| e.index).collect::<Vec<_>>(),
            [0, 1, 3]
        );
        assert!(s.errors[0].to_string().starts_with("action 1 (Helix): "));
        assert_eq!(
            s.messages
                .iter()
                .map(|m| (m.at_ms, m.action))
                .collect::<Vec<_>>(),
            [(0, 2), (5, 4)]
        );
    }

    #[test]
    fn empty_part_and_zero_gap() {
        assert_eq!(
            schedule(&[], ScheduleOptions::default()),
            Schedule::default()
        );
        let s = schedule(
            &[action(
                "X",
                MidiMessage::ProgramChange {
                    channel: 16,
                    program: 127,
                    bank_msb: Some(1),
                    bank_lsb: Some(2),
                },
            )],
            ScheduleOptions::default(),
        );
        assert_eq!(s.messages.len(), 3);
        assert!(s.messages.iter().all(|m| m.at_ms == 0));
    }
}
