//! MIDI 1.0 byte streams: splitting them into messages and decoding those.
//!
//! [`StreamParser`] turns raw bytes, as they arrive from a port in packets of
//! any size, into complete messages. [`Event`] decodes one complete message.

use std::fmt;

/// Default limit for one SysEx message, `F0` and `F7` included (1 MiB).
pub const DEFAULT_MAX_SYSEX_LEN: usize = 1 << 20;

/// Splits a raw MIDI 1.0 byte stream into complete messages.
///
/// - SysEx (`F0 … F7`) is assembled across any number of [`feed`](Self::feed)
///   calls, so a message spread over several packets comes out in one piece.
/// - System real-time bytes (`F8`–`FF`) may appear anywhere, even inside a
///   SysEx or between a status byte and its data. They are emitted on their
///   own, immediately, and do not disturb the message in progress.
/// - Running status is supported for channel messages; system common
///   messages and SysEx clear it.
/// - A SysEx cut short by another status byte is dropped (as browsers do);
///   the new status byte is then handled normally.
/// - A SysEx longer than the limit is dropped as a whole, so memory stays
///   bounded whatever arrives.
/// - Data bytes without a status, stray `F7` and undefined system common
///   bytes (`F4`, `F5`) are ignored.
///
/// Use one parser per input port.
#[derive(Debug, Clone)]
pub struct StreamParser {
    max_sysex: usize,
    /// Running status; 0 = none.
    status: u8,
    expected: usize,
    data: [u8; 2],
    data_len: usize,
    in_sysex: bool,
    overflow: bool,
    sysex: Vec<u8>,
}

impl Default for StreamParser {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamParser {
    /// A parser with the default SysEx limit ([`DEFAULT_MAX_SYSEX_LEN`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_sysex_len(DEFAULT_MAX_SYSEX_LEN)
    }

    /// A parser that drops SysEx messages longer than `max` bytes
    /// (`F0` and `F7` included; at least 2).
    #[must_use]
    pub fn with_max_sysex_len(max: usize) -> Self {
        Self {
            max_sysex: max.max(2),
            status: 0,
            expected: 0,
            data: [0; 2],
            data_len: 0,
            in_sysex: false,
            overflow: false,
            sysex: Vec::new(),
        }
    }

    /// The SysEx limit of this parser.
    #[must_use]
    pub fn max_sysex_len(&self) -> usize {
        self.max_sysex
    }

    /// Forgets any message in progress and the running status.
    pub fn reset(&mut self) {
        self.status = 0;
        self.expected = 0;
        self.data_len = 0;
        self.end_sysex();
    }

    /// Feeds bytes and calls `emit` with each complete message, in order.
    pub fn feed(&mut self, bytes: &[u8], mut emit: impl FnMut(&[u8])) {
        for &b in bytes {
            self.feed_byte(b, &mut emit);
        }
    }

    /// Feeds bytes and returns the complete messages.
    pub fn messages(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        self.feed(bytes, |m| out.push(m.to_vec()));
        out
    }

    /// Feeds bytes and returns the decoded events. Complete messages that are
    /// not a known [`Event`] (undefined real-time bytes `F9`, `FD`) are skipped.
    pub fn events(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut out = Vec::new();
        self.feed(bytes, |m| out.extend(Event::from_message(m)));
        out
    }

    fn end_sysex(&mut self) {
        self.in_sysex = false;
        self.overflow = false;
        self.sysex.clear();
        // Do not keep a huge buffer around after a large message.
        if self.sysex.capacity() > 4096 {
            self.sysex = Vec::new();
        }
    }

    fn feed_byte(&mut self, b: u8, emit: &mut impl FnMut(&[u8])) {
        // Real-time: always a message of its own, never changes any state.
        if b >= 0xF8 {
            emit(&[b]);
            return;
        }

        if self.in_sysex {
            if b < 0x80 {
                if !self.overflow {
                    // Leave room for the closing F7.
                    if self.sysex.len() + 1 < self.max_sysex {
                        self.sysex.push(b);
                    } else {
                        self.overflow = true;
                        self.sysex = Vec::new();
                    }
                }
                return;
            }
            if b == 0xF7 {
                if !self.overflow {
                    self.sysex.push(0xF7);
                    emit(&self.sysex);
                }
                self.end_sysex();
                return;
            }
            // Any other status byte ends the SysEx without an F7: drop it,
            // then handle this byte.
            self.end_sysex();
        }

        match b {
            0xF0 => {
                self.in_sysex = true;
                self.overflow = false;
                self.sysex.clear();
                self.sysex.push(0xF0);
                self.status = 0;
                self.data_len = 0;
            }
            0xF7 => {} // stray end of exclusive
            0x80..=0xEF => {
                self.status = b;
                self.data_len = 0;
                self.expected = if matches!(b & 0xF0, 0xC0 | 0xD0) {
                    1
                } else {
                    2
                };
            }
            0xF1 | 0xF3 => {
                self.status = b;
                self.expected = 1;
                self.data_len = 0;
            }
            0xF2 => {
                self.status = b;
                self.expected = 2;
                self.data_len = 0;
            }
            0xF6 => {
                self.status = 0;
                self.data_len = 0;
                emit(&[b]);
            }
            0xF4 | 0xF5 => {
                // undefined system common: ignore, and it clears running status
                self.status = 0;
                self.data_len = 0;
            }
            _ => {
                // data byte
                if self.status == 0 {
                    return; // no status to attach it to
                }
                if let Some(slot) = self.data.get_mut(self.data_len) {
                    *slot = b;
                    self.data_len += 1;
                }
                if self.data_len >= self.expected {
                    let msg = [self.status, self.data[0], self.data[1]];
                    emit(&msg[..=self.expected.min(2)]);
                    self.data_len = 0;
                    if self.status >= 0xF0 {
                        self.status = 0; // system common has no running status
                    }
                }
            }
        }
    }
}

/// One decoded MIDI 1.0 message.
///
/// Channels are 1–16 as musicians count them (the model crate does the
/// same); all other numbers are wire values. A Note On with velocity 0 is
/// decoded as [`Event::NoteOff`] with velocity 0, as MIDI 1.0 defines it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    PolyPressure {
        channel: u8,
        note: u8,
        pressure: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    ChannelPressure {
        channel: u8,
        pressure: u8,
    },
    /// 14-bit value, 8192 = centre.
    PitchBend {
        channel: u8,
        value: u16,
    },
    /// MIDI Time Code quarter frame (`F1`), the data byte as sent.
    MtcQuarterFrame(u8),
    /// Song position pointer (`F2`) in MIDI beats (sixteenth notes).
    SongPosition(u16),
    SongSelect(u8),
    TuneRequest,
    Clock,
    Start,
    Continue,
    Stop,
    ActiveSensing,
    Reset,
    /// A complete SysEx message, `F0` and `F7` included.
    SysEx(Vec<u8>),
}

impl Event {
    /// Decodes one complete message (as [`StreamParser`] emits them).
    /// Returns `None` for anything that is not exactly one known message.
    #[must_use]
    pub fn from_message(msg: &[u8]) -> Option<Self> {
        let (&status, data) = msg.split_first()?;
        if data.iter().any(|b| *b > 0x7F) && status != 0xF0 {
            return None;
        }
        let channel = (status & 0x0F) + 1;
        let one = || match data {
            [a] => Some(*a),
            _ => None,
        };
        let two = || match data {
            [a, b] => Some((*a, *b)),
            _ => None,
        };
        Some(match status {
            0x80..=0x8F => {
                let (note, velocity) = two()?;
                Self::NoteOff {
                    channel,
                    note,
                    velocity,
                }
            }
            0x90..=0x9F => {
                let (note, velocity) = two()?;
                if velocity == 0 {
                    Self::NoteOff {
                        channel,
                        note,
                        velocity: 0,
                    }
                } else {
                    Self::NoteOn {
                        channel,
                        note,
                        velocity,
                    }
                }
            }
            0xA0..=0xAF => {
                let (note, pressure) = two()?;
                Self::PolyPressure {
                    channel,
                    note,
                    pressure,
                }
            }
            0xB0..=0xBF => {
                let (controller, value) = two()?;
                Self::ControlChange {
                    channel,
                    controller,
                    value,
                }
            }
            0xC0..=0xCF => Self::ProgramChange {
                channel,
                program: one()?,
            },
            0xD0..=0xDF => Self::ChannelPressure {
                channel,
                pressure: one()?,
            },
            0xE0..=0xEF => {
                let (lsb, msb) = two()?;
                Self::PitchBend {
                    channel,
                    value: u16::from(lsb) | u16::from(msb) << 7,
                }
            }
            0xF0 => {
                let (&last, body) = data.split_last()?;
                if last != 0xF7 || body.iter().any(|b| *b > 0x7F) {
                    return None;
                }
                Self::SysEx(msg.to_vec())
            }
            0xF1 => Self::MtcQuarterFrame(one()?),
            0xF2 => {
                let (lsb, msb) = two()?;
                Self::SongPosition(u16::from(lsb) | u16::from(msb) << 7)
            }
            0xF3 => Self::SongSelect(one()?),
            _ if !data.is_empty() => return None,
            0xF6 => Self::TuneRequest,
            0xF8 => Self::Clock,
            0xFA => Self::Start,
            0xFB => Self::Continue,
            0xFC => Self::Stop,
            0xFE => Self::ActiveSensing,
            0xFF => Self::Reset,
            _ => return None,
        })
    }

    /// The wire bytes of this event. Out-of-range values are masked to
    /// their bit width (channel to 1–16, data to 7 or 14 bits), so this never fails.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let ch = |c: u8| c.wrapping_sub(1) & 0x0F;
        let d = |v: u8| v & 0x7F;
        let lsb = |v: u16| (v & 0x7F) as u8;
        let msb = |v: u16| ((v >> 7) & 0x7F) as u8;
        match self {
            Self::NoteOff {
                channel,
                note,
                velocity,
            } => vec![0x80 | ch(*channel), d(*note), d(*velocity)],
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => vec![0x90 | ch(*channel), d(*note), d(*velocity)],
            Self::PolyPressure {
                channel,
                note,
                pressure,
            } => vec![0xA0 | ch(*channel), d(*note), d(*pressure)],
            Self::ControlChange {
                channel,
                controller,
                value,
            } => vec![0xB0 | ch(*channel), d(*controller), d(*value)],
            Self::ProgramChange { channel, program } => vec![0xC0 | ch(*channel), d(*program)],
            Self::ChannelPressure { channel, pressure } => {
                vec![0xD0 | ch(*channel), d(*pressure)]
            }
            Self::PitchBend { channel, value } => {
                vec![0xE0 | ch(*channel), lsb(*value), msb(*value)]
            }
            Self::MtcQuarterFrame(v) => vec![0xF1, d(*v)],
            Self::SongPosition(v) => vec![0xF2, lsb(*v), msb(*v)],
            Self::SongSelect(v) => vec![0xF3, d(*v)],
            Self::TuneRequest => vec![0xF6],
            Self::Clock => vec![0xF8],
            Self::Start => vec![0xFA],
            Self::Continue => vec![0xFB],
            Self::Stop => vec![0xFC],
            Self::ActiveSensing => vec![0xFE],
            Self::Reset => vec![0xFF],
            Self::SysEx(bytes) => bytes.clone(),
        }
    }

    /// The channel (1–16) of a channel message.
    #[must_use]
    pub fn channel(&self) -> Option<u8> {
        match self {
            Self::NoteOff { channel, .. }
            | Self::NoteOn { channel, .. }
            | Self::PolyPressure { channel, .. }
            | Self::ControlChange { channel, .. }
            | Self::ProgramChange { channel, .. }
            | Self::ChannelPressure { channel, .. }
            | Self::PitchBend { channel, .. } => Some(*channel),
            _ => None,
        }
    }

    /// Whether this is a system real-time message (clock, start, stop…).
    #[must_use]
    pub fn is_realtime(&self) -> bool {
        matches!(
            self,
            Self::Clock
                | Self::Start
                | Self::Continue
                | Self::Stop
                | Self::ActiveSensing
                | Self::Reset
        )
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoteOff { channel, note, .. } => write!(f, "Note off {note}, ch {channel}"),
            Self::NoteOn {
                channel,
                note,
                velocity,
            } => write!(f, "Note on {note} vel {velocity}, ch {channel}"),
            Self::PolyPressure {
                channel,
                note,
                pressure,
            } => write!(f, "Poly pressure {note} = {pressure}, ch {channel}"),
            Self::ControlChange {
                channel,
                controller,
                value,
            } => write!(f, "CC {controller} = {value}, ch {channel}"),
            Self::ProgramChange { channel, program } => {
                write!(f, "Program {program}, ch {channel}")
            }
            Self::ChannelPressure { channel, pressure } => {
                write!(f, "Channel pressure {pressure}, ch {channel}")
            }
            Self::PitchBend { channel, value } => write!(f, "Pitch bend {value}, ch {channel}"),
            Self::MtcQuarterFrame(v) => write!(f, "MTC quarter frame {v:02X}"),
            Self::SongPosition(v) => write!(f, "Song position {v}"),
            Self::SongSelect(v) => write!(f, "Song select {v}"),
            Self::TuneRequest => f.write_str("Tune request"),
            Self::Clock => f.write_str("Clock"),
            Self::Start => f.write_str("Start"),
            Self::Continue => f.write_str("Continue"),
            Self::Stop => f.write_str("Stop"),
            Self::ActiveSensing => f.write_str("Active sensing"),
            Self::Reset => f.write_str("Reset"),
            Self::SysEx(bytes) => write!(f, "SysEx ({} bytes)", bytes.len()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut p = StreamParser::new();
        chunks.iter().flat_map(|c| p.messages(c)).collect()
    }

    // A real Play Acoustic exchange: identity reply and a 22 (value) reply.
    const IDENTITY_REPLY: [u8; 14] = [
        0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0x38, 0x70, 0x00, 0x01, 0x04, 0x00, 0xF7,
    ];
    const VALUE_REPLY: [u8; 14] = [
        0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x22, 0x01, 0x39, 0x00, 0x00, 0x00, 0x05, 0xF7,
    ];

    #[test]
    fn sysex_in_one_packet() {
        assert_eq!(parse(&[&IDENTITY_REPLY]), vec![IDENTITY_REPLY.to_vec()]);
    }

    #[test]
    fn sysex_split_at_every_position() {
        for cut in 1..VALUE_REPLY.len() {
            let (a, b) = VALUE_REPLY.split_at(cut);
            assert_eq!(parse(&[a, b]), vec![VALUE_REPLY.to_vec()], "split at {cut}");
        }
    }

    #[test]
    fn sysex_one_byte_per_packet() {
        let chunks: Vec<&[u8]> = VALUE_REPLY.chunks(1).collect();
        assert_eq!(parse(&chunks), vec![VALUE_REPLY.to_vec()]);
    }

    #[test]
    fn large_sysex_across_many_packets() {
        let mut msg = vec![0xF0];
        msg.extend((0..5000).map(|i| (i % 128) as u8));
        msg.push(0xF7);
        let chunks: Vec<&[u8]> = msg.chunks(3).collect();
        assert_eq!(parse(&chunks), vec![msg.clone()]);
    }

    #[test]
    fn realtime_inside_sysex_is_emitted_on_its_own() {
        let mut stream = VALUE_REPLY.to_vec();
        stream.insert(5, 0xF8); // timing clock in the middle
        stream.insert(9, 0xFE); // active sensing too
        let (a, b) = stream.split_at(7);
        assert_eq!(
            parse(&[a, b]),
            vec![vec![0xF8], vec![0xFE], VALUE_REPLY.to_vec()]
        );
    }

    #[test]
    fn realtime_between_status_and_data() {
        assert_eq!(
            parse(&[&[0x90, 0xF8, 0x3C], &[0xFA, 0x64]]),
            vec![vec![0xF8], vec![0xFA], vec![0x90, 0x3C, 0x64]]
        );
    }

    #[test]
    fn running_status() {
        assert_eq!(
            parse(&[&[0x90, 0x3C, 0x64, 0x3E, 0x64], &[0x40, 0x00]]),
            vec![
                vec![0x90, 0x3C, 0x64],
                vec![0x90, 0x3E, 0x64],
                vec![0x90, 0x40, 0x00]
            ]
        );
    }

    #[test]
    fn program_change_and_channel_pressure_take_one_data_byte() {
        assert_eq!(
            parse(&[&[0xC0, 0x05, 0x06, 0xD1, 0x7F]]),
            vec![vec![0xC0, 0x05], vec![0xC0, 0x06], vec![0xD1, 0x7F]]
        );
    }

    #[test]
    fn mixed_sequence() {
        let mut stream = vec![0xC0, 0x02];
        stream.extend(IDENTITY_REPLY);
        stream.extend([0xB0, 0x07, 0x64]);
        stream.extend(VALUE_REPLY);
        assert_eq!(
            parse(&[&stream]),
            vec![
                vec![0xC0, 0x02],
                IDENTITY_REPLY.to_vec(),
                vec![0xB0, 0x07, 0x64],
                VALUE_REPLY.to_vec()
            ]
        );
    }

    #[test]
    fn sysex_after_running_status_does_not_reuse_it() {
        // SysEx clears running status: the data bytes after it have no status and are dropped.
        assert_eq!(
            parse(&[&[0x90, 0x3C, 0x64, 0xF0, 0x7D, 0xF7, 0x3C, 0x64]]),
            vec![vec![0x90, 0x3C, 0x64], vec![0xF0, 0x7D, 0xF7]]
        );
    }

    #[test]
    fn sysex_interrupted_by_status_is_dropped() {
        assert_eq!(
            parse(&[&[0xF0, 0x00, 0x01, 0x38], &[0x90, 0x3C, 0x64]]),
            vec![vec![0x90, 0x3C, 0x64]]
        );
        assert_eq!(
            parse(&[&[0xF0, 0x00, 0x01, 0xF0, 0x7D, 0x01, 0xF7]]),
            vec![vec![0xF0, 0x7D, 0x01, 0xF7]]
        );
    }

    #[test]
    fn stray_bytes_are_ignored() {
        assert_eq!(
            parse(&[&[0x3C, 0x64, 0xF7, 0xF4, 0x01, 0xC0, 0x01]]),
            vec![vec![0xC0, 0x01]]
        );
    }

    #[test]
    fn system_common_has_no_running_status() {
        assert_eq!(
            parse(&[&[0xF2, 0x10, 0x20, 0x30, 0xF3, 0x05, 0x06, 0xF6, 0xF1, 0x11]]),
            vec![
                vec![0xF2, 0x10, 0x20],
                vec![0xF3, 0x05],
                vec![0xF6],
                vec![0xF1, 0x11]
            ]
        );
    }

    #[test]
    fn oversized_sysex_is_dropped_and_parsing_continues() {
        let mut huge = vec![0xF0];
        huge.extend(std::iter::repeat_n(0x01, DEFAULT_MAX_SYSEX_LEN + 10));
        huge.push(0xF7);
        assert_eq!(parse(&[&huge, &[0xC0, 0x03]]), vec![vec![0xC0, 0x03]]);
    }

    #[test]
    fn sysex_limit_is_exact() {
        let mut p = StreamParser::with_max_sysex_len(6);
        let fits = [0xF0, 1, 2, 3, 4, 0xF7];
        assert_eq!(p.messages(&fits), vec![fits.to_vec()]);
        assert!(p.messages(&[0xF0, 1, 2, 3, 4, 5, 0xF7]).is_empty());
        assert_eq!(p.messages(&[0xF8]), vec![vec![0xF8]]);
    }

    #[test]
    fn reset() {
        let mut p = StreamParser::new();
        let mut out = p.messages(&[0xF0, 0x01, 0x02]);
        p.reset();
        out.extend(p.messages(&[0x03, 0xF7, 0xC0, 0x01]));
        assert_eq!(out, vec![vec![0xC0, 0x01]]);
    }

    #[test]
    fn events_are_decoded() {
        let mut p = StreamParser::new();
        let bytes = [
            0x91, 0x3C, 0x64, 0x3C, 0x00, 0xB2, 0x07, 0x64, 0xCF, 0x05, 0xE0, 0x00, 0x40, 0xF2,
            0x01, 0x01, 0xF3, 0x07, 0xF8, 0xFA, 0xFC, 0xFB, 0xF9, 0xA0, 1, 2, 0xD3, 9,
        ];
        assert_eq!(
            p.events(&bytes),
            vec![
                Event::NoteOn {
                    channel: 2,
                    note: 60,
                    velocity: 100
                },
                Event::NoteOff {
                    channel: 2,
                    note: 60,
                    velocity: 0
                },
                Event::ControlChange {
                    channel: 3,
                    controller: 7,
                    value: 100
                },
                Event::ProgramChange {
                    channel: 16,
                    program: 5
                },
                Event::PitchBend {
                    channel: 1,
                    value: 8192
                },
                Event::SongPosition(129),
                Event::SongSelect(7),
                Event::Clock,
                Event::Start,
                Event::Stop,
                Event::Continue,
                Event::PolyPressure {
                    channel: 1,
                    note: 1,
                    pressure: 2
                },
                Event::ChannelPressure {
                    channel: 4,
                    pressure: 9
                },
            ]
        );
        assert_eq!(
            p.events(&IDENTITY_REPLY),
            vec![Event::SysEx(IDENTITY_REPLY.to_vec())]
        );
    }

    #[test]
    fn events_round_trip_through_bytes() {
        let events = [
            Event::NoteOn {
                channel: 16,
                note: 127,
                velocity: 1,
            },
            Event::NoteOff {
                channel: 1,
                note: 0,
                velocity: 64,
            },
            Event::PitchBend {
                channel: 9,
                value: 16383,
            },
            Event::SongPosition(1000),
            Event::MtcQuarterFrame(0x31),
            Event::TuneRequest,
            Event::ActiveSensing,
            Event::Reset,
            Event::SysEx(VALUE_REPLY.to_vec()),
        ];
        for e in events {
            assert_eq!(Event::from_message(&e.to_bytes()), Some(e.clone()), "{e}");
        }
    }

    #[test]
    fn malformed_messages_are_not_events() {
        for bad in [
            &[][..],
            &[0x90, 0x3C],
            &[0x90, 0x3C, 0x64, 0x00],
            &[0xC0],
            &[0xF8, 0x00],
            &[0xF9],
            &[0xF0, 0x01],
            &[0xF0, 0x80, 0xF7],
            &[0x90, 0x80, 0x10],
            &[0x3C],
        ] {
            assert_eq!(Event::from_message(bad), None, "{bad:02X?}");
        }
    }

    #[test]
    fn garbage_never_panics() {
        // A small deterministic pseudo-random stream through a parser with a small limit.
        let mut p = StreamParser::with_max_sysex_len(16);
        let mut x: u32 = 0x1234_5678;
        let mut count = 0;
        for _ in 0..20_000 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            let b = (x >> 24) as u8;
            p.feed(&[b], |m| {
                assert!(!m.is_empty() && m.len() <= 16);
                count += 1;
            });
        }
        assert!(count > 0);
    }
}
