//! The universal MIDI identity request and its reply.

/// Universal identity request to every device on the port: `F0 7E 7F 06 01 F7`.
pub const IDENTITY_REQUEST: [u8; 6] = [0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7];

/// A device's answer to [`IDENTITY_REQUEST`]:
/// `F0 7E <device> 06 02 <manufacturer> <family> <member> <version> F7`.
///
/// Some devices send a shortened reply (the Play Acoustic sends one version
/// byte instead of four); missing family/member bytes read as 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityReply {
    /// Device ID / channel byte the device answered with.
    pub device_id: u8,
    /// Manufacturer ID: one byte, or three starting with `00`.
    pub manufacturer: Vec<u8>,
    /// Device family code, 14 bits (LSB first on the wire).
    pub family: u16,
    /// Family member (model) code, 14 bits.
    pub member: u16,
    /// Software revision bytes as sent (normally 4).
    pub version: Vec<u8>,
}

impl IdentityReply {
    /// Reads an identity reply from one complete SysEx message.
    #[must_use]
    pub fn parse(msg: &[u8]) -> Option<Self> {
        let body = msg.strip_suffix(&[0xF7])?;
        let [0xF0, 0x7E, device_id, 0x06, 0x02, rest @ ..] = body else {
            return None;
        };
        if *device_id > 0x7F || rest.iter().any(|b| *b > 0x7F) {
            return None;
        }
        let (manufacturer, rest) = match rest {
            [0x00, a, b, rest @ ..] => (vec![0x00, *a, *b], rest),
            [m, rest @ ..] if *m != 0x00 => (vec![*m], rest),
            _ => return None,
        };
        // At least the family code's first byte must be there.
        let (&f0, rest) = rest.split_first()?;
        let byte = |i: usize| rest.get(i).copied().unwrap_or(0);
        let family = u16::from(f0) | u16::from(byte(0)) << 7;
        let member = u16::from(byte(1)) | u16::from(byte(2)) << 7;
        let version = rest.get(3..).unwrap_or_default().to_vec();
        Some(Self {
            device_id: *device_id,
            manufacturer,
            family,
            member,
            version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_acoustic_reply() {
        let m = [
            0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0x38, 0x70, 0x00, 0x01, 0x04, 0x00, 0xF7,
        ];
        let id = IdentityReply::parse(&m).unwrap();
        assert_eq!(id.device_id, 0);
        assert_eq!(id.manufacturer, [0x00, 0x01, 0x38]);
        assert_eq!(id.family, 0x70);
        assert_eq!(id.member, 0x01 | 0x04 << 7);
        assert_eq!(id.version, [0x00]);
    }

    #[test]
    fn one_byte_manufacturer_and_full_reply() {
        let m = [
            0xF0, 0x7E, 0x10, 0x06, 0x02, 0x41, 0x0B, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
            0xF7,
        ];
        let id = IdentityReply::parse(&m).unwrap();
        assert_eq!(id.manufacturer, [0x41]);
        assert_eq!(id.family, 0x10B);
        assert_eq!(id.version, [0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn not_identity_replies() {
        for bad in [
            &IDENTITY_REQUEST[..],
            &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0xF7],
            &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x41, 0xF7],
            &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0x38, 0x70],
            &[0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0x38, 0x80, 0xF7],
            &[],
        ] {
            assert_eq!(IdentityReply::parse(bad), None, "{bad:02X?}");
        }
    }
}
