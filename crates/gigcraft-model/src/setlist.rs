use crate::{Extra, SetlistId, SongId, schema_version};
use serde::{Deserialize, Serialize};

/// A setlist for a gig or rehearsal: one or more sets in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Setlist {
    #[serde(default = "schema_version")]
    pub schema: u32,
    pub id: SetlistId,
    pub name: String,
    /// Date of the gig as `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(default)]
    pub sets: Vec<Set>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One set (main set, encore…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Set {
    pub name: String,
    #[serde(default)]
    pub entries: Vec<Entry>,
}

/// One line of a set: a song or a break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entry {
    Song(SongEntry),
    Break {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        label: String,
        minutes: u32,
    },
}

/// A song in a set, with overrides for this gig only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongEntry {
    pub song: SongId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tempo_bpm: Option<f64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// Played only if there is time or demand.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

impl SongEntry {
    /// An entry without overrides.
    #[must_use]
    pub fn new(song: SongId) -> Self {
        Self {
            song,
            key: None,
            tempo_bpm: None,
            notes: String::new(),
            optional: false,
        }
    }
}

/// Running times of a setlist.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetlistTiming {
    /// Seconds per set, songs and breaks (optional songs not counted).
    pub per_set: Vec<u32>,
    pub total: u32,
    /// Seconds of optional songs, on top of `total`.
    pub optional: u32,
    /// Songs without a known duration.
    pub unknown: usize,
}

impl Setlist {
    /// A new, empty setlist with one set.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: crate::SCHEMA_VERSION,
            id: SetlistId::new(),
            name: name.into(),
            date: None,
            notes: String::new(),
            sets: vec![Set {
                name: "Set 1".into(),
                entries: Vec::new(),
            }],
            extra: Extra::new(),
        }
    }

    /// All song entries in performance order.
    pub fn song_entries(&self) -> impl Iterator<Item = &SongEntry> {
        self.sets
            .iter()
            .flat_map(|s| s.entries.iter())
            .filter_map(|e| match e {
                Entry::Song(s) => Some(s),
                Entry::Break { .. } => None,
            })
    }

    /// Running times, looking up each song's duration.
    pub fn timing(&self, duration_of: impl Fn(SongId) -> Option<u32>) -> SetlistTiming {
        let mut t = SetlistTiming::default();
        for set in &self.sets {
            let mut secs: u32 = 0;
            for entry in &set.entries {
                match entry {
                    Entry::Song(e) => match duration_of(e.song) {
                        Some(d) if e.optional => t.optional = t.optional.saturating_add(d),
                        Some(d) => secs = secs.saturating_add(d),
                        None => t.unknown += 1,
                    },
                    Entry::Break { minutes, .. } => {
                        secs = secs.saturating_add(minutes.saturating_mul(60));
                    }
                }
            }
            t.per_set.push(secs);
            t.total = t.total.saturating_add(secs);
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_counts_breaks_optional_and_unknown() {
        let a = SongId::new();
        let b = SongId::new();
        let c = SongId::new();
        let mut list = Setlist::new("Summer fest");
        list.sets[0].entries = vec![
            Entry::Song(SongEntry::new(a)),
            Entry::Break {
                label: "Change".into(),
                minutes: 2,
            },
            Entry::Song(SongEntry {
                optional: true,
                ..SongEntry::new(b)
            }),
        ];
        list.sets.push(Set {
            name: "Encore".into(),
            entries: vec![Entry::Song(SongEntry::new(c))],
        });
        let t = list.timing(|id| if id == c { None } else { Some(200) });
        assert_eq!(t.per_set, vec![320, 0]);
        assert_eq!(t.total, 320);
        assert_eq!(t.optional, 200);
        assert_eq!(t.unknown, 1);
        assert_eq!(list.song_entries().count(), 3);
    }

    #[test]
    fn toml_round_trip_with_breaks_and_same_song_twice() {
        let a = SongId::new();
        let mut list = Setlist::new("Wedding");
        list.date = Some("2026-12-31".into());
        list.sets[0].entries = vec![
            Entry::Song(SongEntry {
                key: Some("A".into()),
                ..SongEntry::new(a)
            }),
            Entry::Break {
                label: String::new(),
                minutes: 15,
            },
            Entry::Song(SongEntry::new(a)),
        ];
        let text = toml::to_string(&list).unwrap_or_default();
        let back: Setlist = toml::from_str(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        assert_eq!(back, list);
    }
}
