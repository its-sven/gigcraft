//! Device profiles: how a device numbers and switches its presets.
//!
//! Profiles are TOML files in the repository's `device-profiles/` folder so
//! the community can add devices. The built-in ones are compiled in
//! ([`builtin`]); users can load more with [`DeviceProfile::from_toml`].

use crate::identity::IdentityReply;
use serde::{Deserialize, Serialize};

/// The profile format version this build reads and writes.
pub const PROFILE_SCHEMA: u32 = 1;

/// Highest preset count a profile may declare (128 banks of 128 programs).
pub const MAX_PRESET_COUNT: u16 = 16384;

fn schema_version() -> u32 {
    PROFILE_SCHEMA
}

/// What GigCraft knows about one kind of device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceProfile {
    #[serde(default = "schema_version")]
    pub schema: u32,
    /// Unique id: lowercase letters, digits and dashes (e.g. `tc-helicon-play-acoustic`).
    pub id: String,
    /// Shown in the app.
    pub name: String,
    pub manufacturer: String,
    /// Model names this profile covers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Number of presets on the device.
    pub preset_count: u16,
    /// How the device shows program numbers (display only; the wire stays 0–127).
    pub numbering: Numbering,
    /// How presets are switched.
    pub switching: Switching,
    /// How the device answers the universal identity request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityMatch>,
    /// Quirks, tips and sources.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// How a device shows program number 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Numbering {
    /// Program 0 is shown as 0.
    Zero,
    /// Program 0 is shown as 1.
    One,
}

impl Numbering {
    /// The number a musician sees for wire value `wire`.
    #[must_use]
    pub fn display(self, wire: u8) -> u16 {
        match self {
            Self::Zero => u16::from(wire),
            Self::One => u16::from(wire) + 1,
        }
    }

    /// The wire value 0–127 for a number as the device shows it.
    #[must_use]
    pub fn wire(self, display: u16) -> Option<u8> {
        let wire = match self {
            Self::Zero => display,
            Self::One => display.checked_sub(1)?,
        };
        u8::try_from(wire).ok().filter(|w| *w <= 127)
    }
}

/// How presets are switched on a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Switching {
    /// Bank Select (optional) and Program Change.
    ProgramChange,
    /// The TC-Helicon Play Acoustic driver ([`crate::play_acoustic`]): Program
    /// Change if the pedal honours it, else stepping with front-panel events.
    PlayAcousticStepping,
}

/// How a device answers the universal identity request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityMatch {
    /// Manufacturer ID: one byte, or three starting with `0x00`.
    pub manufacturer: Vec<u8>,
    /// Accepted family codes and the model each means; empty = any family.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub family: Vec<FamilyMatch>,
}

/// One device family code and the model it identifies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyMatch {
    /// Family code, 14 bits.
    pub code: u16,
    pub model: String,
}

impl IdentityMatch {
    /// Whether `reply` comes from a device this profile covers.
    #[must_use]
    pub fn matches(&self, reply: &IdentityReply) -> bool {
        reply.manufacturer == self.manufacturer
            && (self.family.is_empty() || self.family.iter().any(|f| f.code == reply.family))
    }

    /// The model name for `reply`, if it matches a listed family.
    #[must_use]
    pub fn model(&self, reply: &IdentityReply) -> Option<&str> {
        if reply.manufacturer != self.manufacturer {
            return None;
        }
        self.family
            .iter()
            .find(|f| f.code == reply.family)
            .map(|f| f.model.as_str())
    }
}

/// A profile that could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("device profile is not valid TOML: {0}")]
    Parse(String),
    #[error("device profile is invalid: {}", .0.join("; "))]
    Invalid(Vec<String>),
    #[error("device profile could not be written: {0}")]
    Write(String),
}

impl DeviceProfile {
    /// Reads and validates a profile from TOML text.
    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        let profile: Self = toml::from_str(text).map_err(|e| ProfileError::Parse(e.to_string()))?;
        profile.validate()?;
        Ok(profile)
    }

    /// The profile as TOML text.
    pub fn to_toml(&self) -> Result<String, ProfileError> {
        toml::to_string(self).map_err(|e| ProfileError::Write(e.to_string()))
    }

    /// Checks everything TOML cannot: id format, ranges, identity bytes.
    /// All problems are reported at once.
    pub fn validate(&self) -> Result<(), ProfileError> {
        let mut problems = Vec::new();
        if self.schema == 0 || self.schema > PROFILE_SCHEMA {
            problems.push(format!(
                "schema {} is not supported (this version reads {PROFILE_SCHEMA})",
                self.schema
            ));
        }
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !self.id.starts_with('-')
            && !self.id.ends_with('-');
        if !id_ok {
            problems.push(format!(
                "id {:?} must be 1–64 lowercase letters, digits and dashes",
                self.id
            ));
        }
        if self.name.trim().is_empty() {
            problems.push("name is empty".into());
        }
        if self.manufacturer.trim().is_empty() {
            problems.push("manufacturer is empty".into());
        }
        if self.models.iter().any(|m| m.trim().is_empty()) {
            problems.push("models contains an empty name".into());
        }
        if !(1..=MAX_PRESET_COUNT).contains(&self.preset_count) {
            problems.push(format!(
                "preset_count {} is outside 1–{MAX_PRESET_COUNT}",
                self.preset_count
            ));
        }
        if self.switching == Switching::PlayAcousticStepping && self.identity.is_none() {
            problems.push(
                "play_acoustic_stepping needs an [identity]: only an identified device is driven"
                    .into(),
            );
        }
        if let Some(identity) = &self.identity {
            let m = &identity.manufacturer;
            let bytes_ok = m.iter().all(|b| *b <= 0x7F);
            let shape_ok = match m.as_slice() {
                [one] => !matches!(*one, 0x00 | 0x7D | 0x7E | 0x7F),
                [0x00, _, _] => true,
                _ => false,
            };
            if !bytes_ok || !shape_ok {
                problems.push(format!(
                    "identity.manufacturer {m:02X?} must be one byte (01–7C) or three starting with 00"
                ));
            }
            for f in &identity.family {
                if f.code > 0x3FFF {
                    problems.push(format!(
                        "identity.family code {:#X} is more than 14 bits",
                        f.code
                    ));
                }
                if f.model.trim().is_empty() {
                    problems.push(format!(
                        "identity.family code {:#X} has no model name",
                        f.code
                    ));
                } else if !self.models.is_empty() && !self.models.contains(&f.model) {
                    problems.push(format!(
                        "identity.family model {:?} is not listed in models",
                        f.model
                    ));
                }
            }
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(ProfileError::Invalid(problems))
        }
    }

    /// Whether `reply` comes from a device this profile covers.
    #[must_use]
    pub fn matches_identity(&self, reply: &IdentityReply) -> bool {
        self.identity.as_ref().is_some_and(|i| i.matches(reply))
    }
}

/// The number a musician sees for program `wire` on a device with `profile`.
#[must_use]
pub fn display_preset_number(profile: &DeviceProfile, wire: u8) -> u16 {
    profile.numbering.display(wire)
}

/// The program number 0–127 to send for a preset number as `profile`'s device
/// shows it; `None` if there is no such program.
#[must_use]
pub fn wire_preset_number(profile: &DeviceProfile, display: u16) -> Option<u8> {
    profile.numbering.wire(display)
}

/// The built-in profiles' sources: file name in `device-profiles/` and TOML text.
pub const BUILTIN_SOURCES: &[(&str, &str)] = &[
    (
        "generic-midi.toml",
        include_str!("../../../device-profiles/generic-midi.toml"),
    ),
    (
        "tc-helicon-play-acoustic.toml",
        include_str!("../../../device-profiles/tc-helicon-play-acoustic.toml"),
    ),
];

/// Id of the built-in generic profile.
pub const GENERIC_MIDI: &str = "generic-midi";
/// Id of the built-in TC-Helicon Play Acoustic / Play Electric profile.
pub const PLAY_ACOUSTIC: &str = "tc-helicon-play-acoustic";

/// The profiles shipped with GigCraft (every one is checked by the tests).
#[must_use]
pub fn builtin() -> Vec<DeviceProfile> {
    BUILTIN_SOURCES
        .iter()
        .filter_map(|(_, text)| DeviceProfile::from_toml(text).ok())
        .collect()
}

/// The built-in profile with `id`.
#[must_use]
pub fn builtin_by_id(id: &str) -> Option<DeviceProfile> {
    builtin().into_iter().find(|p| p.id == id)
}

/// The first profile in `profiles` whose identity matches `reply`.
#[must_use]
pub fn find_by_identity<'a>(
    profiles: &'a [DeviceProfile],
    reply: &IdentityReply,
) -> Option<&'a DeviceProfile> {
    profiles.iter().find(|p| p.matches_identity(reply))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_profile_is_valid_and_unique() {
        for (file, text) in BUILTIN_SOURCES {
            let p = DeviceProfile::from_toml(text).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(format!("{}.toml", p.id), *file, "file named after the id");
        }
        let all = builtin();
        assert_eq!(all.len(), BUILTIN_SOURCES.len());
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
    }

    #[test]
    fn play_acoustic_profile() {
        let p = builtin_by_id(PLAY_ACOUSTIC).unwrap();
        assert_eq!(p.switching, Switching::PlayAcousticStepping);
        assert_eq!(p.numbering, Numbering::One);
        assert_eq!(p.models, ["Play Acoustic", "Play Electric"]);
        assert!(p.notes.contains("never writes parameters"));
        let reply = |family: u8| {
            IdentityReply::parse(&[
                0xF0, 0x7E, 0x00, 0x06, 0x02, 0x00, 0x01, 0x38, family, 0x00, 0x01, 0x04, 0x00,
                0xF7,
            ])
            .unwrap()
        };
        let id = p.identity.as_ref().unwrap();
        assert_eq!(id.model(&reply(0x70)), Some("Play Acoustic"));
        assert_eq!(id.model(&reply(0x71)), Some("Play Electric"));
        assert!(!p.matches_identity(&reply(0x72)));
        let all = builtin();
        assert_eq!(
            find_by_identity(&all, &reply(0x71)).map(|p| p.id.as_str()),
            Some(PLAY_ACOUSTIC)
        );
        assert_eq!(find_by_identity(&all, &reply(0x10)), None);
    }

    #[test]
    fn generic_profile() {
        let p = builtin_by_id(GENERIC_MIDI).unwrap();
        assert_eq!(p.switching, Switching::ProgramChange);
        assert_eq!(p.preset_count, 128);
        assert_eq!(display_preset_number(&p, 0), 0);
        assert!(p.identity.is_none());
    }

    #[test]
    fn display_and_wire_numbers() {
        let mut p = builtin_by_id(GENERIC_MIDI).unwrap();
        p.numbering = Numbering::One;
        assert_eq!(display_preset_number(&p, 0), 1);
        assert_eq!(display_preset_number(&p, 127), 128);
        assert_eq!(wire_preset_number(&p, 1), Some(0));
        assert_eq!(wire_preset_number(&p, 128), Some(127));
        assert_eq!(wire_preset_number(&p, 0), None);
        assert_eq!(wire_preset_number(&p, 129), None);
        assert_eq!(Numbering::Zero.wire(0), Some(0));
        assert_eq!(Numbering::Zero.wire(128), None);
        assert_eq!(Numbering::Zero.wire(u16::MAX), None);
        // the wire is stored, the display follows the profile
        assert_eq!(display_preset_number(&p, 200), 201);
    }

    #[test]
    fn round_trip_through_toml() {
        for p in builtin() {
            let text = p.to_toml().unwrap();
            assert_eq!(DeviceProfile::from_toml(&text).unwrap(), p);
        }
    }

    #[test]
    fn unknown_fields_are_tolerated() {
        let text = r#"
            id = "my-synth"
            name = "My synth"
            manufacturer = "Me"
            preset_count = 64
            numbering = "one"
            switching = "program_change"
            future_field = true
        "#;
        let p = DeviceProfile::from_toml(text).unwrap();
        assert_eq!(p.schema, PROFILE_SCHEMA);
        assert_eq!(p.notes, "");
    }

    #[test]
    fn invalid_profiles_are_reported() {
        assert!(matches!(
            DeviceProfile::from_toml("id = 3"),
            Err(ProfileError::Parse(_))
        ));
        assert!(matches!(
            DeviceProfile::from_toml(
                "id='a'\nname='b'\nmanufacturer='c'\npreset_count=1\nnumbering='two'\nswitching='program_change'"
            ),
            Err(ProfileError::Parse(_))
        ));
        let text = r#"
            schema = 9
            id = "Bad Id"
            name = " "
            manufacturer = ""
            models = ["Box"]
            preset_count = 0
            numbering = "zero"
            switching = "play_acoustic_stepping"
        "#;
        let Err(ProfileError::Invalid(problems)) = DeviceProfile::from_toml(text) else {
            panic!("expected problems");
        };
        assert_eq!(problems.len(), 6, "{problems:#?}");

        let bad_identity = r#"
            id = "box"
            name = "Box"
            manufacturer = "Me"
            models = ["Box"]
            preset_count = 10
            numbering = "zero"
            switching = "program_change"
            [identity]
            manufacturer = [0x7E]
            [[identity.family]]
            code = 0x4000
            model = "Other"
        "#;
        let Err(ProfileError::Invalid(problems)) = DeviceProfile::from_toml(bad_identity) else {
            panic!("expected problems");
        };
        assert_eq!(problems.len(), 3, "{problems:#?}");
        let message = ProfileError::Invalid(problems).to_string();
        assert!(message.contains("14 bits"), "{message}");
    }
}
