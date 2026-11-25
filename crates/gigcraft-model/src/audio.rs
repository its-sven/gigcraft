//! Backing-track settings of a song: audio tracks, the click and markers.
//!
//! All of it is optional; a song without audio settings writes none of these
//! keys, so files from before Phase 2 round-trip unchanged.

use serde::{Deserialize, Serialize};

/// Highest output bus (stereo pair) a track or the click may name: bus 31 is
/// outputs 63/64.
pub const MAX_BUS: u32 = 31;

/// Lowest and highest gain a track or the click may have, in dB.
pub const GAIN_DB_RANGE: std::ops::RangeInclusive<f64> = -96.0..=24.0;

/// Most count-in bars.
pub const MAX_COUNT_IN_BARS: u32 = 16;

/// One audio file played with the song (a stem, the backing track, a click
/// or guide-vocal track).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Name of a file in the song's attachments.
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub role: TrackRole,
    /// Gain in dB (0 = unchanged).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain_db: f64,
    /// −1 hard left … 0 centre … 1 hard right.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pan: f64,
    /// Output bus: stereo pair index (0 = outputs 1/2, 1 = outputs 3/4, …).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub bus: u32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mute: bool,
}

impl Track {
    /// A backing track for `file` at unity gain, centred, on bus 0.
    #[must_use]
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            label: String::new(),
            role: TrackRole::default(),
            gain_db: 0.0,
            pan: 0.0,
            bus: 0,
            mute: false,
        }
    }
}

/// What a track is for. It only labels the track; routing is by `bus`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackRole {
    #[default]
    Backing,
    Click,
    Guide,
    Stem,
}

/// The built-in click. Tempo and beats per bar come from the song's
/// `tempo_bpm` and `time_signature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClickSettings {
    /// Click during the song (the count-in clicks either way).
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Output bus of the click (stereo pair index), usually the in-ears.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub bus: u32,
    /// Bars clicked before the song starts (0 = none).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub count_in_bars: u32,
    /// Click level in dB.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub volume_db: f64,
}

impl Default for ClickSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            bus: 0,
            count_in_bars: 0,
            volume_db: 0.0,
        }
    }
}

/// A named point in the backing tracks, or a region when it has an end.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marker {
    pub name: String,
    /// Start, in seconds from the start of the tracks.
    pub at_secs: f64,
    /// End of the region, if this marker is one (needed to loop it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_secs: Option<f64>,
}

fn yes() -> bool {
    true
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde passes a reference"
)]
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde passes a reference"
)]
fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

/// Beats per bar from a time signature as written ("4/4" → 4, "6/8" → 6).
#[must_use]
pub fn beats_per_bar(time_signature: &str) -> Option<u32> {
    let (num, den) = time_signature.trim().split_once('/')?;
    let num: u32 = num.trim().parse().ok()?;
    let den: u32 = den.trim().parse().ok()?;
    ((1..=32).contains(&num) && matches!(den, 1 | 2 | 4 | 8 | 16 | 32)).then_some(num)
}
