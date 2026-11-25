use serde::{Deserialize, Serialize};

/// A MIDI message as stored in a song part.
///
/// Channels are 1–16 as musicians count them. Program, controller, note and
/// value numbers are the wire values 0–127; how a device numbers its presets
/// (from 0 or from 1) is a display question for its device profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MidiMessage {
    ProgramChange {
        channel: u8,
        program: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bank_msb: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bank_lsb: Option<u8>,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
    },
    /// System exclusive, as hex text including `F0` and `F7` (e.g. `"F0 00 20 1F F7"`).
    SysEx {
        hex: String,
    },
    SongSelect {
        song: u8,
    },
}

impl MidiMessage {
    /// The raw bytes of this message, or a description of what is wrong.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        fn ch(channel: u8) -> Result<u8, String> {
            if (1..=16).contains(&channel) {
                Ok(channel - 1)
            } else {
                Err(format!("MIDI channel {channel} is outside 1–16"))
            }
        }
        fn data(name: &str, v: u8) -> Result<u8, String> {
            if v <= 127 {
                Ok(v)
            } else {
                Err(format!("{name} {v} is outside 0–127"))
            }
        }
        Ok(match self {
            Self::ProgramChange {
                channel,
                program,
                bank_msb,
                bank_lsb,
            } => {
                let c = ch(*channel)?;
                let mut out = Vec::with_capacity(8);
                if let Some(msb) = bank_msb {
                    out.extend([0xB0 | c, 0, data("bank MSB", *msb)?]);
                }
                if let Some(lsb) = bank_lsb {
                    out.extend([0xB0 | c, 32, data("bank LSB", *lsb)?]);
                }
                out.extend([0xC0 | c, data("program", *program)?]);
                out
            }
            Self::ControlChange {
                channel,
                controller,
                value,
            } => {
                vec![
                    0xB0 | ch(*channel)?,
                    data("controller", *controller)?,
                    data("value", *value)?,
                ]
            }
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => {
                vec![
                    0x90 | ch(*channel)?,
                    data("note", *note)?,
                    data("velocity", *velocity)?,
                ]
            }
            Self::NoteOff { channel, note } => vec![0x80 | ch(*channel)?, data("note", *note)?, 0],
            Self::SysEx { hex } => parse_sysex(hex)?,
            Self::SongSelect { song } => vec![0xF3, data("song", *song)?],
        })
    }
}

impl std::fmt::Display for MidiMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProgramChange {
                channel,
                program,
                bank_msb,
                bank_lsb,
            } => {
                write!(f, "Program {program}")?;
                if bank_msb.is_some() || bank_lsb.is_some() {
                    write!(
                        f,
                        " (bank {}/{})",
                        bank_msb.unwrap_or(0),
                        bank_lsb.unwrap_or(0)
                    )?;
                }
                write!(f, ", ch {channel}")
            }
            Self::ControlChange {
                channel,
                controller,
                value,
            } => write!(f, "CC {controller} = {value}, ch {channel}"),
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => write!(f, "Note on {note} vel {velocity}, ch {channel}"),
            Self::NoteOff { channel, note } => write!(f, "Note off {note}, ch {channel}"),
            Self::SysEx { hex } => write!(f, "SysEx {hex}"),
            Self::SongSelect { song } => write!(f, "Song select {song}"),
        }
    }
}

fn parse_sysex(hex: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err("SysEx hex has an odd number of digits".into());
    }
    let bytes = (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            cleaned
                .get(i..i + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| format!("SysEx contains invalid hex near position {i}"))
        })
        .collect::<Result<Vec<u8>, String>>()?;
    match (bytes.first(), bytes.last()) {
        (Some(0xF0), Some(0xF7)) if bytes.len() >= 2 => {}
        _ => return Err("SysEx must start with F0 and end with F7".into()),
    }
    if bytes[1..bytes.len() - 1].iter().any(|b| *b > 0x7F) {
        return Err("SysEx data bytes must be 00–7F".into());
    }
    Ok(bytes)
}

/// A MIDI message for a named device (an output port or device profile).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MidiAction {
    /// Device name as configured on this setup (e.g. "Play Acoustic").
    pub device: String,
    #[serde(flatten)]
    pub message: MidiMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_change_with_banks() {
        let m = MidiMessage::ProgramChange {
            channel: 2,
            program: 5,
            bank_msb: Some(1),
            bank_lsb: Some(3),
        };
        assert_eq!(m.to_bytes(), Ok(vec![0xB1, 0, 1, 0xB1, 32, 3, 0xC1, 5]));
    }

    #[test]
    fn rejects_bad_values() {
        let bad_channel = MidiMessage::ControlChange {
            channel: 0,
            controller: 7,
            value: 100,
        };
        assert!(bad_channel.to_bytes().is_err());
        let bad_value = MidiMessage::ControlChange {
            channel: 1,
            controller: 7,
            value: 200,
        };
        assert!(bad_value.to_bytes().is_err());
    }

    #[test]
    fn sysex_is_checked() {
        let ok = MidiMessage::SysEx {
            hex: "F0 00 20 1F 7F F7".into(),
        };
        assert_eq!(ok.to_bytes(), Ok(vec![0xF0, 0x00, 0x20, 0x1F, 0x7F, 0xF7]));
        for bad in ["F0 00", "00 F7", "F0 80 F7", "F0 0 F7", "F0 ZZ F7", ""] {
            assert!(
                MidiMessage::SysEx { hex: bad.into() }.to_bytes().is_err(),
                "{bad}"
            );
        }
    }
}
