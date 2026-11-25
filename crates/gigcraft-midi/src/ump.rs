//! Universal MIDI Packets (UMP) carrying MIDI 1.0, as CoreMIDI's
//! protocol-1.0 input ports and `MIDISendEventList` use them.
//!
//! Only what a MIDI 1.0 app needs: system messages (type 1), MIDI 1.0
//! channel voice (type 2) and 7-bit SysEx (type 3), always on group 0 when
//! sending. Other message types are skipped with their correct length.

/// Number of 32-bit words in a UMP, from the message type in its first word.
#[must_use]
pub fn word_count(first_word: u32) -> usize {
    match first_word >> 28 {
        0x0 | 0x1 | 0x2 | 0x6 | 0x7 => 1,
        0x3 | 0x4 | 0x8 | 0x9 | 0xA => 2,
        0xB | 0xC => 3,
        _ => 4, // 0x5, 0xD, 0xE, 0xF
    }
}

/// Length of a MIDI 1.0 message (status byte included) that starts with
/// `status`, for messages that fit in one UMP word; 0 for SysEx and
/// undefined bytes.
#[must_use]
pub fn short_message_len(status: u8) -> usize {
    match status {
        0x80..=0xBF | 0xE0..=0xEF | 0xF2 => 3,
        0xC0..=0xDF | 0xF1 | 0xF3 => 2,
        0xF6 | 0xF8..=0xFF => 1,
        _ => 0,
    }
}

/// Appends the MIDI 1.0 bytes carried by `words` (any number of UMPs) to `out`.
///
/// SysEx arrives as a series of 7-bit packets (start / continue / end, or
/// complete); they become `F0 … F7` bytes here, and reassembly across calls
/// is left to [`StreamParser`](crate::parser::StreamParser). A packet cut
/// off at the end of `words` is ignored.
pub fn append_bytes(words: &[u32], out: &mut Vec<u8>) {
    let mut i = 0;
    while let Some(&w0) = words.get(i) {
        let n = word_count(w0);
        match w0 >> 28 {
            0x1 | 0x2 => {
                let status = ((w0 >> 16) & 0xFF) as u8;
                let len = short_message_len(status);
                if len > 0 {
                    out.push(status);
                    if len > 1 {
                        out.push(((w0 >> 8) & 0x7F) as u8);
                    }
                    if len > 2 {
                        out.push((w0 & 0x7F) as u8);
                    }
                }
            }
            0x3 => {
                let Some(&w1) = words.get(i + 1) else {
                    return;
                };
                let kind = (w0 >> 20) & 0xF; // 0 complete, 1 start, 2 continue, 3 end
                let count = ((w0 >> 16) & 0xF).min(6) as usize;
                let payload = [
                    (w0 >> 8) as u8,
                    w0 as u8,
                    (w1 >> 24) as u8,
                    (w1 >> 16) as u8,
                    (w1 >> 8) as u8,
                    w1 as u8,
                ];
                if kind == 0 || kind == 1 {
                    out.push(0xF0);
                }
                out.extend(payload.iter().take(count).map(|b| b & 0x7F));
                if kind == 0 || kind == 3 {
                    out.push(0xF7);
                }
            }
            _ => {}
        }
        i += n;
    }
}

/// The MIDI 1.0 bytes carried by `words`.
#[must_use]
pub fn to_bytes(words: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    append_bytes(words, &mut out);
    out
}

/// The single UMP word (group 0) for one complete MIDI 1.0 message that is
/// not SysEx: channel voice, system common or real-time. `None` if `bytes`
/// is not exactly one such message.
#[must_use]
pub fn word_for_short_message(bytes: &[u8]) -> Option<u32> {
    let &status = bytes.first()?;
    if status < 0x80 || status == 0xF0 || status == 0xF7 {
        return None;
    }
    let len = short_message_len(status);
    if len == 0 || bytes.len() != len {
        return None;
    }
    let kind: u32 = if status < 0xF0 { 0x2 } else { 0x1 };
    let d1 = bytes.get(1).map_or(0, |b| u32::from(b & 0x7F));
    let d2 = bytes.get(2).map_or(0, |b| u32::from(b & 0x7F));
    Some(kind << 28 | u32::from(status) << 16 | d1 << 8 | d2)
}

/// The SysEx7 UMPs (group 0, 6 data bytes per packet) for one complete SysEx
/// message `F0 … F7`. `None` if `bytes` is not one, or has a data byte above `7F`.
#[must_use]
pub fn sysex_words(bytes: &[u8]) -> Option<Vec<u32>> {
    let body = bytes.strip_prefix(&[0xF0])?.strip_suffix(&[0xF7])?;
    if body.iter().any(|b| *b > 0x7F) {
        return None;
    }
    let chunks: Vec<&[u8]> = if body.is_empty() {
        vec![&[][..]]
    } else {
        body.chunks(6).collect()
    };
    let last = chunks.len() - 1;
    let mut words = Vec::with_capacity(chunks.len() * 2);
    for (i, c) in chunks.iter().enumerate() {
        let kind: u32 = match (i, last) {
            (_, 0) => 0,
            (0, _) => 1,
            (i, l) if i == l => 3,
            _ => 2,
        };
        let mut b = [0u8; 6];
        for (slot, v) in b.iter_mut().zip(c.iter()) {
            *slot = *v;
        }
        let [b0, b1, b2, b3, b4, b5] = b.map(u32::from);
        words.push(0x3 << 28 | kind << 20 | (c.len() as u32) << 16 | b0 << 8 | b1);
        words.push(b2 << 24 | b3 << 16 | b4 << 8 | b5);
    }
    Some(words)
}

/// The UMP words for any number of complete MIDI 1.0 messages in `bytes`
/// (split with a fresh [`StreamParser`](crate::parser::StreamParser)).
#[must_use]
pub fn words_for_bytes(bytes: &[u8]) -> Vec<u32> {
    let mut words = Vec::new();
    crate::parser::StreamParser::new().feed(bytes, |m| {
        if m.first() == Some(&0xF0) {
            words.extend(sysex_words(m).into_iter().flatten());
        } else {
            words.extend(word_for_short_message(m));
        }
    });
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::StreamParser;

    const VALUE_REPLY: [u8; 14] = [
        0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x22, 0x01, 0x39, 0x00, 0x00, 0x00, 0x05, 0xF7,
    ];

    #[test]
    fn complete_sysex_in_one_packet() {
        let msg = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
        let words = sysex_words(&msg).unwrap();
        assert_eq!(words.len(), 2);
        assert_eq!(to_bytes(&words), msg);
    }

    #[test]
    fn multi_packet_sysex() {
        let words = sysex_words(&VALUE_REPLY).unwrap();
        assert_eq!(words.len(), 4);
        assert_eq!(words[0] >> 20 & 0xF, 1, "start");
        assert_eq!(words[2] >> 20 & 0xF, 3, "end");
        assert_eq!(to_bytes(&words), VALUE_REPLY);
        let mut long = vec![0xF0];
        long.extend(0..20u8);
        long.push(0xF7);
        assert_eq!(to_bytes(&sysex_words(&long).unwrap()), long);
        assert_eq!(to_bytes(&sysex_words(&[0xF0, 0xF7]).unwrap()), [0xF0, 0xF7]);
    }

    #[test]
    fn sysex_packets_split_across_event_packets_reassemble() {
        // Each UMP in its own packet, with a clock between them: the parser still yields one SysEx.
        let words = sysex_words(&VALUE_REPLY).unwrap();
        let mut parser = StreamParser::new();
        let mut out = Vec::new();
        for pair in words.chunks(2) {
            out.extend(parser.messages(&to_bytes(pair)));
            out.extend(parser.messages(&to_bytes(&[0x10F8_0000]))); // type 1 timing clock
        }
        assert_eq!(
            out.iter().filter(|m| **m != [0xF8]).collect::<Vec<_>>(),
            vec![&VALUE_REPLY.to_vec()]
        );
        assert_eq!(
            out.iter().filter(|m| **m == [0xF8]).count(),
            words.len() / 2
        );
    }

    #[test]
    fn channel_voice_and_system_messages() {
        assert_eq!(to_bytes(&[0x2090_3C64]), [0x90, 0x3C, 0x64]);
        assert_eq!(to_bytes(&[0x20C0_0500]), [0xC0, 0x05]);
        assert_eq!(to_bytes(&[0x10F2_1020]), [0xF2, 0x10, 0x20]);
        assert_eq!(to_bytes(&[0x10FE_0000]), [0xFE]);
    }

    #[test]
    fn other_message_types_are_skipped_with_their_length() {
        // utility (1 word), MIDI 2.0 voice (2 words), data 128 (4 words), then a note on
        let words = [
            0x0000_0000,
            0x4090_3C00,
            0xFFFF_0000,
            0x5000_0000,
            1,
            2,
            3,
            0x2090_3C64,
        ];
        assert_eq!(to_bytes(&words), [0x90, 0x3C, 0x64]);
    }

    #[test]
    fn truncated_packets_are_ignored() {
        assert_eq!(to_bytes(&[0x3010_0000]), Vec::<u8>::new());
        assert_eq!(to_bytes(&[0x2090_3C64, 0x4090_3C00]), [0x90, 0x3C, 0x64]);
        // An undefined status in a type 2 word carries nothing.
        assert_eq!(to_bytes(&[0x2070_0000]), Vec::<u8>::new());
    }

    #[test]
    fn short_message_words() {
        assert_eq!(
            word_for_short_message(&[0x90, 0x3C, 0x64]),
            Some(0x2090_3C64)
        );
        assert_eq!(word_for_short_message(&[0xC0, 0x05]), Some(0x20C0_0500));
        assert_eq!(word_for_short_message(&[0xF8]), Some(0x10F8_0000));
        assert_eq!(word_for_short_message(&[0x90, 0x3C]), None); // incomplete
        assert_eq!(word_for_short_message(&[0xF0, 0x01, 0xF7]), None); // SysEx goes another way
        assert_eq!(word_for_short_message(&[0x3C]), None);
        assert_eq!(word_for_short_message(&[]), None);
        for m in [
            &[0x90, 0x3C, 0x64][..],
            &[0xC0, 0x05],
            &[0xF2, 0x01, 0x02],
            &[0xFF],
        ] {
            assert_eq!(to_bytes(&[word_for_short_message(m).unwrap()]), m);
        }
    }

    #[test]
    fn bad_sysex_has_no_words() {
        assert_eq!(sysex_words(&[0xF0, 0x80, 0xF7]), None);
        assert_eq!(sysex_words(&[0xF0, 0x01]), None);
        assert_eq!(sysex_words(&[0x90, 0x01, 0xF7]), None);
    }

    #[test]
    fn words_for_mixed_bytes() {
        let mut bytes = vec![0xB0, 0x00, 0x01, 0xC0, 0x05];
        bytes.extend(VALUE_REPLY);
        let words = words_for_bytes(&bytes);
        assert_eq!(to_bytes(&words), bytes);
    }
}
