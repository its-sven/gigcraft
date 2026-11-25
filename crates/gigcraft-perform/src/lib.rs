//! Perform mode: what is on stage right now.
//!
//! A [`Show`] is a setlist flattened into a running order of songs and
//! breaks, with a position (item and song part). Stage [`Action`]s move the
//! position and return [`Effect`]s for the UI and the MIDI layer to carry out
//! (send this part's presets, turn a page, stop everything). Nothing here does
//! I/O and nothing depends on time, so the same logic drives every platform
//! and can be tested completely.
//!
//! [`Keymap`] maps keys (computer keyboards, Bluetooth page turners) with a
//! gesture (press, double press, hold) to actions.

mod keymap;

pub use keymap::{Binding, Gesture, Keymap};

use gigcraft_model::{Entry, Setlist, SetlistId, Song, SongId};
use serde::{Deserialize, Serialize};

/// Something the musician (or a pedal, MIDI message, remote) asks for on stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Next part of the song; after the last part, the next song.
    Next,
    /// Previous part; before the first part, the previous song (at its last part).
    Previous,
    NextSong,
    PreviousSong,
    /// Scroll the chart forward by a page (UI).
    PageForward,
    PageBack,
    /// Send the current part's presets again.
    Resend,
    /// All notes off, stop audio (later), cancel pending preset changes.
    Panic,
    /// Leave perform mode.
    Exit,
}

impl Action {
    pub const ALL: [Action; 9] = [
        Action::Next,
        Action::Previous,
        Action::NextSong,
        Action::PreviousSong,
        Action::PageForward,
        Action::PageBack,
        Action::Resend,
        Action::Panic,
        Action::Exit,
    ];

    /// Human-readable name.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Action::Next => "Next",
            Action::Previous => "Previous",
            Action::NextSong => "Next song",
            Action::PreviousSong => "Previous song",
            Action::PageForward => "Page forward",
            Action::PageBack => "Page back",
            Action::Resend => "Send presets again",
            Action::Panic => "Panic",
            Action::Exit => "Exit perform mode",
        }
    }
}

/// What the platform should do after an action.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Effect {
    /// Show item `index` (a song or a break).
    Show {
        index: usize,
    },
    /// Send the MIDI actions of this song part.
    SendPart {
        song: SongId,
        part: usize,
    },
    /// A break starts: show a countdown.
    Break {
        minutes: u32,
    },
    PageForward,
    PageBack,
    Panic,
    Exit,
    /// Already at the start or end; nothing changed.
    Edge,
}

/// One position in the running order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Song {
        song: SongId,
        title: String,
        artist: String,
        /// Key for this gig (override) or the song's key.
        key: Option<String>,
        tempo_bpm: Option<f64>,
        /// Note for this gig only.
        gig_notes: String,
        part_names: Vec<String>,
        optional: bool,
        set: usize,
        /// Number within the set, from 1.
        number: usize,
    },
    Break {
        label: String,
        minutes: u32,
        set: usize,
    },
    /// A setlist entry whose song is no longer in the library.
    Missing { song: SongId, set: usize },
}

/// The running order of a setlist and the current position.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Show {
    pub setlist: SetlistId,
    pub name: String,
    pub set_names: Vec<String>,
    pub items: Vec<Item>,
    pub index: usize,
    pub part: usize,
}

impl Show {
    /// Flatten `setlist`, looking songs up with `song`. Starts at the first item.
    pub fn new<'a>(setlist: &Setlist, song: impl Fn(SongId) -> Option<&'a Song>) -> Self {
        let mut items = Vec::new();
        for (si, set) in setlist.sets.iter().enumerate() {
            let mut number = 0;
            for entry in &set.entries {
                match entry {
                    Entry::Song(e) => match song(e.song) {
                        Some(s) => {
                            number += 1;
                            items.push(Item::Song {
                                song: s.id,
                                title: s.title.clone(),
                                artist: s.artist.clone(),
                                key: e.key.clone().or_else(|| s.key.clone()),
                                tempo_bpm: e.tempo_bpm.or(s.tempo_bpm),
                                gig_notes: e.notes.clone(),
                                part_names: s.parts.iter().map(|p| p.name.clone()).collect(),
                                optional: e.optional,
                                set: si,
                                number,
                            });
                        }
                        None => items.push(Item::Missing {
                            song: e.song,
                            set: si,
                        }),
                    },
                    Entry::Break { label, minutes } => {
                        items.push(Item::Break {
                            label: label.clone(),
                            minutes: *minutes,
                            set: si,
                        });
                    }
                }
            }
        }
        Self {
            setlist: setlist.id,
            name: setlist.name.clone(),
            set_names: setlist.sets.iter().map(|s| s.name.clone()).collect(),
            items,
            index: 0,
            part: 0,
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&Item> {
        self.items.get(self.index)
    }

    /// The next item after the current one (for "up next" displays).
    #[must_use]
    pub fn up_next(&self) -> Option<&Item> {
        self.items.get(self.index + 1)
    }

    fn part_count(&self, index: usize) -> usize {
        match self.items.get(index) {
            Some(Item::Song { part_names, .. }) => part_names.len().max(1),
            _ => 1,
        }
    }

    /// Effects for arriving at the current position.
    fn arrive(&self) -> Vec<Effect> {
        let mut out = vec![Effect::Show { index: self.index }];
        match self.current() {
            Some(Item::Song { song, .. }) => out.push(Effect::SendPart {
                song: *song,
                part: self.part,
            }),
            Some(Item::Break { minutes, .. }) => out.push(Effect::Break { minutes: *minutes }),
            _ => {}
        }
        out
    }

    /// Effects for the start of the show (first item).
    #[must_use]
    pub fn start(&self) -> Vec<Effect> {
        if self.items.is_empty() {
            vec![Effect::Edge]
        } else {
            self.arrive()
        }
    }

    /// Jump to item `index`, part `part` (clamped).
    pub fn goto(&mut self, index: usize, part: usize) -> Vec<Effect> {
        if self.items.is_empty() {
            return vec![Effect::Edge];
        }
        self.index = index.min(self.items.len() - 1);
        self.part = part.min(self.part_count(self.index) - 1);
        self.arrive()
    }

    /// Apply a stage action.
    pub fn apply(&mut self, action: Action) -> Vec<Effect> {
        if self.items.is_empty() {
            return match action {
                Action::Panic => vec![Effect::Panic],
                Action::Exit => vec![Effect::Exit],
                _ => vec![Effect::Edge],
            };
        }
        match action {
            Action::Next => {
                if self.part + 1 < self.part_count(self.index) {
                    self.part += 1;
                    self.part_changed()
                } else {
                    self.apply(Action::NextSong)
                }
            }
            Action::Previous => {
                if self.part > 0 {
                    self.part -= 1;
                    self.part_changed()
                } else if self.index > 0 {
                    self.index -= 1;
                    self.part = self.part_count(self.index) - 1;
                    self.arrive()
                } else {
                    vec![Effect::Edge]
                }
            }
            Action::NextSong => {
                if self.index + 1 < self.items.len() {
                    self.index += 1;
                    self.part = 0;
                    self.arrive()
                } else {
                    vec![Effect::Edge]
                }
            }
            Action::PreviousSong => {
                if self.index > 0 {
                    self.index -= 1;
                    self.part = 0;
                    self.arrive()
                } else if self.part > 0 {
                    // First song: back to its start.
                    self.part = 0;
                    self.part_changed()
                } else {
                    vec![Effect::Edge]
                }
            }
            Action::PageForward => vec![Effect::PageForward],
            Action::PageBack => vec![Effect::PageBack],
            Action::Resend => self.part_changed(),
            Action::Panic => vec![Effect::Panic],
            Action::Exit => vec![Effect::Exit],
        }
    }

    fn part_changed(&self) -> Vec<Effect> {
        match self.current() {
            Some(Item::Song { song, .. }) => vec![Effect::SendPart {
                song: *song,
                part: self.part,
            }],
            _ => Vec::new(),
        }
    }

    /// Index of the item `song` first appears at, if any.
    #[must_use]
    pub fn find(&self, song: SongId) -> Option<usize> {
        self.items
            .iter()
            .position(|i| matches!(i, Item::Song { song: s, .. } if *s == song))
    }
}

#[cfg(test)]
mod tests;
