//! The GigCraft data model.
//!
//! Plain data types with no I/O: songs with their parts and MIDI actions,
//! setlists with sets, song entries and breaks, and the library manifest.
//! Every record carries a schema version and keeps fields it does not know
//! (`extra`), so files written by a newer version survive a round trip.

mod id;
mod midi;
mod setlist;
mod song;
mod validate;

pub use id::{LibraryId, SetlistId, SongId};
pub use midi::{MidiAction, MidiMessage};
pub use setlist::{Entry, Set, Setlist, SetlistTiming, SongEntry};
pub use song::{Attachment, AttachmentKind, Link, LinkKind, Part, Song};
pub use validate::{Issue, Validate};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema version written into every record by this build.
pub const SCHEMA_VERSION: u32 = 1;

/// Fields a record does not know, kept for round trips.
pub type Extra = BTreeMap<String, serde_json::Value>;

/// The manifest at the root of a library folder (`gigcraft-library.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryInfo {
    #[serde(default = "schema_version")]
    pub schema: u32,
    pub id: LibraryId,
    pub name: String,
    #[serde(flatten)]
    pub extra: Extra,
}

impl LibraryInfo {
    /// A new manifest with a fresh id.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            id: LibraryId::new(),
            name: name.into(),
            extra: Extra::new(),
        }
    }
}

pub(crate) fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// Format seconds as `m:ss` or `h:mm:ss`.
#[must_use]
pub fn format_duration(seconds: u32) -> String {
    let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Parse `m:ss`, `h:mm:ss` or plain seconds.
#[must_use]
pub fn parse_duration(text: &str) -> Option<u32> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    let nums: Option<Vec<u32>> = parts.iter().map(|p| p.trim().parse::<u32>().ok()).collect();
    let nums = nums?;
    match nums.as_slice() {
        [s] => Some(*s),
        [m, s] if *s < 60 => m.checked_mul(60)?.checked_add(*s),
        [h, m, s] if *m < 60 && *s < 60 => h.checked_mul(3600)?.checked_add(m * 60 + s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_round_trip() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(245), "4:05");
        assert_eq!(format_duration(3725), "1:02:05");
        assert_eq!(parse_duration("4:05"), Some(245));
        assert_eq!(parse_duration("1:02:05"), Some(3725));
        assert_eq!(parse_duration("90"), Some(90));
        assert_eq!(parse_duration("4:75"), None);
        assert_eq!(parse_duration("x"), None);
        assert_eq!(parse_duration(""), None);
    }
}
