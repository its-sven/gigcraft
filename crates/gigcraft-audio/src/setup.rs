//! What the host hands the engine for one song.

use crate::{Clip, LoadError};
use gigcraft_model::{Song, Track};

/// One track of a song: a clip with its mix settings.
#[derive(Debug, Clone)]
pub struct TrackSetup {
    pub clip: Clip,
    /// Gain in dB.
    pub gain_db: f32,
    /// −1 hard left … 1 hard right. Mono clips pan with a −3 dB centre,
    /// stereo clips balance (0 dB centre).
    pub pan: f32,
    /// Output bus: stereo pair index (0 = outputs 1/2, 1 = outputs 3/4).
    pub bus: usize,
    pub mute: bool,
    pub solo: bool,
}

impl TrackSetup {
    /// A clip at unity gain, centred, on bus 0.
    #[must_use]
    pub fn new(clip: Clip) -> Self {
        Self {
            clip,
            gain_db: 0.0,
            pan: 0.0,
            bus: 0,
            mute: false,
            solo: false,
        }
    }
}

/// Tempo grid of a song: drives the click, the count-in and jumps at the
/// next bar. Bar 1 starts at the first frame of the tracks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    /// Clicks per minute (the beat of the time signature: 6/8 at 120 is 120 eighths).
    pub bpm: f64,
    pub beats_per_bar: u32,
}

/// The built-in click. It needs a [`Tempo`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickSetup {
    /// Click during the song. The count-in clicks either way.
    pub enabled: bool,
    /// Bars clicked before the tracks start when playing with count-in.
    pub count_in_bars: u32,
    /// Output bus (stereo pair index), usually the in-ears.
    pub bus: usize,
    /// Level in dB; 0 dB clicks peak at about −3 dBFS on the downbeat.
    pub gain_db: f32,
    /// A higher, louder click on beat 1.
    pub accent: bool,
}

impl Default for ClickSetup {
    fn default() -> Self {
        Self {
            enabled: false,
            count_in_bars: 0,
            bus: 0,
            gain_db: 0.0,
            accent: true,
        }
    }
}

/// A named point, or a region when it has an end.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerSetup {
    pub name: String,
    pub start_secs: f64,
    pub end_secs: Option<f64>,
}

/// Everything the engine plays for one song. With no tracks it is a
/// metronome that runs until stopped.
#[derive(Debug, Clone, Default)]
pub struct SongSetup {
    pub tracks: Vec<TrackSetup>,
    pub tempo: Option<Tempo>,
    pub click: ClickSetup,
    pub markers: Vec<MarkerSetup>,
}

impl SongSetup {
    /// The setup for a library song. `clip_for` loads a track's file (the
    /// host reads it from the song folder and calls [`Clip::decode`]).
    /// Tempo comes from `tempo_bpm` and `time_signature` (4 beats a bar when
    /// none is written).
    ///
    /// # Errors
    /// The first track whose clip could not be loaded, with its file name.
    pub fn from_song(
        song: &Song,
        mut clip_for: impl FnMut(&Track) -> Result<Clip, LoadError>,
    ) -> Result<Self, LoadError> {
        let mut tracks = Vec::with_capacity(song.tracks.len());
        for t in &song.tracks {
            let clip = clip_for(t).map_err(|e| LoadError::Track {
                file: t.file.clone(),
                source: Box::new(e),
            })?;
            tracks.push(TrackSetup {
                clip,
                gain_db: t.gain_db as f32,
                pan: t.pan as f32,
                bus: t.bus as usize,
                mute: t.mute,
                solo: false,
            });
        }
        let tempo = song
            .tempo_bpm
            .filter(|b| b.is_finite() && *b > 0.0)
            .map(|bpm| Tempo {
                bpm,
                beats_per_bar: song
                    .time_signature
                    .as_deref()
                    .and_then(gigcraft_model::beats_per_bar)
                    .unwrap_or(4),
            });
        let click = song
            .click
            .as_ref()
            .map(|c| ClickSetup {
                enabled: c.enabled,
                count_in_bars: c.count_in_bars,
                bus: c.bus as usize,
                gain_db: c.volume_db as f32,
                accent: true,
            })
            .unwrap_or_default();
        let markers = song
            .markers
            .iter()
            .map(|m| MarkerSetup {
                name: m.name.clone(),
                start_secs: m.at_secs,
                end_secs: m.end_secs,
            })
            .collect();
        Ok(Self {
            tracks,
            tempo,
            click,
            markers,
        })
    }
}
