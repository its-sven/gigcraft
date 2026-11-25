//! The GigCraft backing-track engine.
//!
//! A pure engine without I/O, threads or a clock: the platform host (cpal on
//! desktop, AVAudioEngine on iOS) owns the audio device and calls
//! [`Renderer::render`] from its callback; the UI drives the engine through
//! the [`Controller`].
//!
//! - **Clips**: audio files decoded into memory as `f32` with craft-audio
//!   (WAV, AIFF, FLAC, MP3, AAC/M4A, Ogg, …) and resampled to the output rate
//!   with craft-audio-dsp, on the loading thread.
//! - **Songs**: several tracks, each with gain, pan, mute/solo and an output
//!   bus (a stereo pair: backing to outputs 1/2, click to 3/4).
//! - **Click**: built-in click and count-in from the song's tempo and time
//!   signature, accent on beat 1, on its own bus, with or without tracks.
//! - **Transport**: play, pause, stop and seek with a short declick fade,
//!   fades in and out, crossfade to the next song, markers and regions (loop
//!   a region, jump to a marker now or on the next bar line).
//!
//! ```
//! use gigcraft_audio::{Clip, EngineConfig, SongSetup, TrackSetup, engine};
//!
//! let (mut control, mut renderer) = engine(EngineConfig::new(48_000));
//! let tone: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
//! let clip = Clip::from_planar(48_000, vec![tone], 48_000)?;
//! control.load(SongSetup { tracks: vec![TrackSetup::new(clip)], ..SongSetup::default() })?;
//! control.play(false)?;
//! let mut out = vec![0.0; 256 * 2];
//! renderer.render(&mut out, 2); // in the host's audio callback
//! assert!(out.iter().any(|s| *s != 0.0));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

mod clip;
mod control;
mod program;
mod render;
mod setup;

pub use clip::{AUDIO_EXTENSIONS, Clip, LoadError, MAX_CLIP_SAMPLES};
pub use control::{
    Controller, EngineConfig, EngineError, Event, JumpAt, MAX_METER_CHANNELS, PlayState,
    SongHandle, Status, engine,
};
pub use render::{DECLICK_MS, Renderer};
pub use setup::{ClickSetup, MarkerSetup, SongSetup, Tempo, TrackSetup};

#[cfg(test)]
mod tests;
