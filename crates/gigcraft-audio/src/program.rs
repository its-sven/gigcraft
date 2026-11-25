//! A song prepared for the renderer. Built on the control thread (where
//! allocating is fine) and moved to the renderer whole.

use crate::{Clip, SongSetup};
use craft_audio_dsp::pan::{PanLaw, gains};

/// Gain and pan changes glide over this long, so they never click.
pub(crate) const PARAM_RAMP_MS: f32 = 10.0;

/// Length of one click sound.
const CLICK_MS: f64 = 30.0;

/// Frequency and peak of the click on beat 1 and on the other beats.
pub(crate) const ACCENT_CLICK: (f64, f64) = (1760.0, 0.7);
pub(crate) const BEAT_CLICK: (f64, f64) = (1320.0, 0.45);

/// A value that moves linearly towards a target over a whole number of
/// frames. Each frame's value is computed from the start, not accumulated, so
/// a ramp of N frames reaches its target exactly after N frames.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ramp {
    pub value: f32,
    pub target: f32,
    from: f32,
    len: u32,
    done: u32,
}

impl Ramp {
    pub(crate) fn at(value: f32) -> Self {
        Self {
            value,
            target: value,
            from: value,
            len: 0,
            done: 0,
        }
    }

    /// Glide from the current value to `target` over `frames` frames (at once when 0).
    pub(crate) fn to(&mut self, target: f32, frames: u32) {
        self.from = self.value;
        self.target = target;
        self.len = frames;
        self.done = 0;
        if frames == 0 {
            self.value = target;
        }
    }

    /// The value for this frame; then one step on.
    #[inline]
    pub(crate) fn tick(&mut self) -> f32 {
        let v = self.value;
        if self.done < self.len {
            self.done += 1;
            self.value = if self.done >= self.len {
                self.target
            } else {
                self.from + (self.target - self.from) * (self.done as f32 / self.len as f32)
            };
        }
        v
    }

    pub(crate) fn settled(&self) -> bool {
        self.done >= self.len
    }
}

pub(crate) fn db_to_gain(db: f32) -> f32 {
    if db.is_finite() {
        10f32.powf(db.clamp(-96.0, 24.0) / 20.0)
    } else {
        0.0
    }
}

/// Converts milliseconds to frames at `rate`, at least 1.
pub(crate) fn ms_to_frames(ms: f32, rate: u32) -> u32 {
    let ms = if ms.is_finite() { ms.max(0.0) } else { 0.0 };
    let f = (f64::from(ms) * f64::from(rate) / 1000.0).round();
    (f as u32).max(1)
}

/// Bars and beats in frames.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grid {
    /// Frames per beat (fractional; each beat starts at the rounded frame).
    pub beat: f64,
    pub beats_per_bar: u32,
}

impl Grid {
    pub(crate) fn beat_start(&self, k: i64) -> i64 {
        (k as f64 * self.beat).round() as i64
    }

    /// The beat sounding at grid position `q`: its index and first frame.
    #[inline]
    pub(crate) fn beat_at(&self, q: i64) -> (i64, i64) {
        let mut k = (q as f64 / self.beat).floor() as i64;
        // Rounding the beat starts can move `q` one beat either way.
        if self.beat_start(k + 1) <= q {
            k += 1;
        } else if self.beat_start(k) > q {
            k -= 1;
        }
        (k, self.beat_start(k))
    }

    pub(crate) fn bar_frames(&self) -> f64 {
        self.beat * f64::from(self.beats_per_bar)
    }

    /// First frame of the first bar that starts at or after `pos`.
    pub(crate) fn next_bar(&self, pos: i64) -> i64 {
        let bar = self.bar_frames();
        let mut k = (pos as f64 / bar).floor() as i64;
        for _ in 0..3 {
            let start = (k as f64 * bar).round() as i64;
            if start >= pos {
                return start;
            }
            k += 1;
        }
        (k as f64 * bar).round() as i64
    }
}

/// A track while it plays: its clip and the gliding left/right gains.
#[derive(Debug)]
pub(crate) struct LiveTrack {
    pub clip: Clip,
    pub gain_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub bus: usize,
    pub left: Ramp,
    pub right: Ramp,
}

impl LiveTrack {
    fn targets(&self, any_solo: bool) -> (f32, f32) {
        if self.mute || (any_solo && !self.solo) {
            return (0.0, 0.0);
        }
        let law = if self.clip.channels() == 1 {
            PanLaw::Minus3
        } else {
            PanLaw::Zero
        };
        let (l, r) = gains(self.pan, law);
        let g = db_to_gain(self.gain_db);
        (l * g, r * g)
    }
}

/// The click while it plays.
#[derive(Debug)]
pub(crate) struct LiveClick {
    pub enabled: bool,
    pub count_in_bars: u32,
    pub bus: usize,
    pub gain: Ramp,
    pub accent: bool,
    pub accent_sound: Vec<f32>,
    pub beat_sound: Vec<f32>,
}

/// A song ready to render.
#[derive(Debug)]
pub(crate) struct Program {
    pub id: u64,
    pub tracks: Vec<LiveTrack>,
    pub grid: Option<Grid>,
    pub click: LiveClick,
    /// Length of the longest track; `None` (no tracks) runs until stopped.
    pub length: Option<i64>,
    pub param_ramp: u32,
}

impl Program {
    pub(crate) fn new(id: u64, setup: SongSetup, rate: u32) -> Self {
        let any_solo = setup.tracks.iter().any(|t| t.solo);
        let mut tracks: Vec<LiveTrack> = setup
            .tracks
            .into_iter()
            .map(|t| LiveTrack {
                clip: t.clip,
                gain_db: t.gain_db,
                pan: t.pan,
                mute: t.mute,
                solo: t.solo,
                bus: t.bus,
                left: Ramp::at(0.0),
                right: Ramp::at(0.0),
            })
            .collect();
        for t in &mut tracks {
            let (l, r) = t.targets(any_solo);
            t.left = Ramp::at(l);
            t.right = Ramp::at(r);
        }
        let length = tracks
            .iter()
            .map(|t| t.clip.frames() as i64)
            .max()
            .filter(|_| !tracks.is_empty());
        let grid = setup
            .tempo
            .filter(|t| t.bpm.is_finite() && (1.0..=1000.0).contains(&t.bpm))
            .map(|t| Grid {
                beat: f64::from(rate) * 60.0 / t.bpm,
                beats_per_bar: t.beats_per_bar.clamp(1, 64),
            });
        let c = setup.click;
        Self {
            id,
            tracks,
            grid,
            click: LiveClick {
                enabled: c.enabled,
                count_in_bars: c.count_in_bars.min(64),
                bus: c.bus,
                gain: Ramp::at(db_to_gain(c.gain_db)),
                accent: c.accent,
                accent_sound: click_sound(ACCENT_CLICK, rate),
                beat_sound: click_sound(BEAT_CLICK, rate),
            },
            length,
            param_ramp: ms_to_frames(PARAM_RAMP_MS, rate),
        }
    }

    pub(crate) fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    /// Point every track's gains at their new targets.
    pub(crate) fn retarget(&mut self) {
        let any_solo = self.any_solo();
        let frames = self.param_ramp;
        for t in &mut self.tracks {
            let (l, r) = t.targets(any_solo);
            t.left.to(l, frames);
            t.right.to(r, frames);
        }
    }

    /// Frames of the count-in, when the song has one.
    pub(crate) fn count_in_frames(&self) -> Option<i64> {
        let grid = self.grid?;
        (self.click.count_in_bars > 0).then(|| {
            grid.beat_start(i64::from(self.click.count_in_bars) * i64::from(grid.beats_per_bar))
        })
    }
}

/// A short sine blip with a fast attack, an exponential decay and a linear
/// release to exactly 0, so it starts and ends without a step.
pub(crate) fn click_sound((freq, amp): (f64, f64), rate: u32) -> Vec<f32> {
    let rate = f64::from(rate);
    let len = (CLICK_MS / 1000.0 * rate).round().max(1.0) as usize;
    let attack = (0.0005 * rate).max(1.0);
    let release = (0.003 * rate).max(1.0);
    (0..len)
        .map(|i| {
            let t = i as f64 / rate;
            let i = i as f64;
            let env = (i / attack).min(1.0)
                * (-t / 0.008).exp()
                * ((len as f64 - 1.0 - i) / release).clamp(0.0, 1.0);
            (amp * env * (std::f64::consts::TAU * freq * t).sin()) as f32
        })
        .collect()
}
