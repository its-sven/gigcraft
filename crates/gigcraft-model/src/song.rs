use crate::{Extra, MidiAction, SongId, schema_version};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A song in the library. The chart (lyrics and chords) is kept beside it as
/// a ChordPro file; this record holds everything else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    #[serde(default = "schema_version")]
    pub schema: u32,
    pub id: SongId,
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// Musical key as written (e.g. "G", "F#m", "Bb").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f64>,
    /// Time signature as written (e.g. "4/4", "6/8").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capo: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Parts in performance order (intro, verse, solo…), each with the MIDI
    /// actions sent when it starts. A song without parts behaves like one part.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    /// Free fields defined by the user ("Singer" = "Anna").
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Song {
    /// A new song with a fresh id and only a title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            schema: crate::SCHEMA_VERSION,
            id: SongId::new(),
            title: title.into(),
            artist: String::new(),
            key: None,
            tempo_bpm: None,
            time_signature: None,
            duration_secs: None,
            capo: None,
            tags: Vec::new(),
            notes: String::new(),
            parts: Vec::new(),
            links: Vec::new(),
            custom: BTreeMap::new(),
            extra: Extra::new(),
        }
    }

    /// "Title – Artist", or just the title.
    #[must_use]
    pub fn display_name(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} – {}", self.title, self.artist)
        }
    }
}

/// A part of a song and the MIDI actions sent when it starts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<MidiAction>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

/// A link stored with a song (chords on Ultimate Guitar, a video, a recording).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// What a link points to, derived from its host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    UltimateGuitar,
    YouTube,
    Spotify,
    Other,
}

impl Link {
    /// The kind of site this link points to.
    #[must_use]
    pub fn kind(&self) -> LinkKind {
        let lower = self.url.to_ascii_lowercase();
        let rest = lower.split_once("://").map_or(lower.as_str(), |(_, r)| r);
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        let is = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
        if is("ultimate-guitar.com") {
            LinkKind::UltimateGuitar
        } else if is("youtube.com") || is("youtu.be") {
            LinkKind::YouTube
        } else if is("spotify.com") {
            LinkKind::Spotify
        } else {
            LinkKind::Other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MidiMessage;

    #[test]
    fn link_kinds_use_the_host_only() {
        let k = |u: &str| {
            Link {
                url: u.into(),
                label: None,
            }
            .kind()
        };
        assert_eq!(
            k("https://tabs.ultimate-guitar.com/tab/x/y-chords-1"),
            LinkKind::UltimateGuitar
        );
        assert_eq!(k("https://youtu.be/abc"), LinkKind::YouTube);
        assert_eq!(k("https://open.spotify.com/track/1"), LinkKind::Spotify);
        assert_eq!(
            k("https://example.com/?u=ultimate-guitar.com"),
            LinkKind::Other
        );
        assert_eq!(k("https://notultimate-guitar.com/"), LinkKind::Other);
    }

    #[test]
    fn toml_round_trip_keeps_unknown_fields() {
        let mut song = Song::new("Harbour Lights");
        song.artist = "The Demo Band".into();
        song.key = Some("G".into());
        song.tempo_bpm = Some(96.0);
        song.parts.push(Part {
            name: "Intro".into(),
            actions: vec![MidiAction {
                device: "Play Acoustic".into(),
                message: MidiMessage::ProgramChange {
                    channel: 1,
                    program: 4,
                    bank_msb: None,
                    bank_lsb: None,
                },
            }],
            notes: String::new(),
        });
        // Top-level keys must come before the first table header.
        let text = format!(
            "future_field = \"kept\"\n{}",
            toml::to_string(&song).unwrap_or_default()
        );
        let back: Song = toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(
            back.extra.get("future_field"),
            Some(&serde_json::json!("kept"))
        );
        let mut without_extra = back.clone();
        without_extra.extra.clear();
        assert_eq!(without_extra, song);
        let again = toml::to_string(&back).unwrap_or_default();
        let twice: Song = toml::from_str(&again).unwrap_or_else(|e| panic!("{e}\n{again}"));
        assert_eq!(twice, back, "second round trip must be lossless:\n{again}");
    }
}
