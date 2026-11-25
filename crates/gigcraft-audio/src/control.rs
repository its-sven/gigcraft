//! The control side: commands from the UI thread, feedback from the renderer.
//!
//! # Why `std::sync::mpsc::sync_channel`
//! The workspace denies `unsafe`, so a hand-written ring buffer is out. A
//! *bounded* std channel (`sync_channel(n)` with `n > 0`) is the array flavour
//! of std's crossbeam-derived channels: its slots are allocated once when the
//! channel is made, and `try_send`/`try_recv` are lock-free compare-and-swap
//! loops on those slots. The only lock in it guards the list of threads
//! *blocked* in `send`/`recv`, and it is skipped while that list is empty,
//! which is always the case here: both sides only ever call `try_send` and
//! `try_recv`. So the renderer neither allocates nor waits on a lock when it
//! reads commands or posts events.
//!
//! Nothing the renderer owns is freed on the audio thread either: a song it
//! lets go of goes back through the `retired` channel and is dropped here, on
//! the control thread. Position, state and meters travel through atomics.

use crate::program::{Program, ms_to_frames};
use crate::{MarkerSetup, Renderer, SongSetup};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

/// Most output channels the meters track.
pub const MAX_METER_CHANNELS: usize = 64;

/// How the engine runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineConfig {
    /// Output sample rate in Hz; clips must be decoded at this rate.
    pub sample_rate: u32,
    /// Most frames rendered in one pass; longer host buffers are split.
    pub max_block_frames: usize,
    /// Commands that can wait for the renderer before `QueueFull`.
    pub queue_capacity: usize,
}

impl EngineConfig {
    /// 4096-frame blocks and room for 256 queued commands.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            max_block_frames: 4096,
            queue_capacity: 256,
        }
    }
}

/// Identifies a loaded song in [`Status`] and [`Event`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SongHandle(pub u64);

/// Where to jump to a marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpAt {
    /// Now, with a few milliseconds of fade so it does not click.
    Now,
    /// Sample-exactly on the next bar line (now when the song has no tempo).
    NextBar,
}

/// Transport state of the current song.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

/// What the renderer reports back; read with [`Controller::status`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status {
    pub song: Option<SongHandle>,
    pub state: PlayState,
    /// Position in the tracks in frames; negative during the count-in.
    pub position_frames: i64,
    pub position_secs: f64,
    pub counting_in: bool,
    pub looping: bool,
    /// A track or the click names a bus the output device does not have.
    pub unrouted: bool,
}

/// Things that happened on the audio thread; read with [`Controller::events`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The count-in is over; the tracks start now.
    CountInFinished(SongHandle),
    /// The song played to its end and stopped (rewound to the start).
    Ended(SongHandle),
    /// A song faded out or was crossfaded away and is silent now.
    FadedOut(SongHandle),
}

/// Why a command was not sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("the audio engine is busy; try again")]
    QueueFull,
    #[error("the audio engine has stopped")]
    Disconnected,
    #[error("no song is loaded")]
    NoSong,
    #[error("the song has no track {0}")]
    NoTrack(usize),
    #[error("the song has no marker {0:?}")]
    UnknownMarker(String),
    #[error("marker {0:?} has no end, so it cannot loop")]
    NotARegion(String),
}

/// One per-track setting.
#[derive(Debug, Clone, Copy)]
pub(crate) enum TrackParam {
    GainDb(f32),
    Pan(f32),
    Mute(bool),
    Solo(bool),
    Bus(usize),
}

/// One click setting.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ClickParam {
    Enabled(bool),
    GainDb(f32),
    Bus(usize),
}

/// Messages to the renderer. Songs travel boxed, so receiving one moves a
/// pointer and never allocates.
#[derive(Debug)]
pub(crate) enum Command {
    Load(Box<Program>),
    Crossfade(Box<Program>, u32),
    Play { count_in: bool },
    Pause,
    Stop,
    Seek(i64),
    FadeIn(u32),
    FadeOut(u32),
    Loop(Option<(i64, i64)>),
    Jump { to: i64, at: JumpAt },
    Track(usize, TrackParam),
    Click(ClickParam),
}

pub(crate) const STATE_MASK: u32 = 0b11;
pub(crate) const COUNTING_IN: u32 = 1 << 2;
pub(crate) const LOOPING: u32 = 1 << 3;
pub(crate) const UNROUTED: u32 = 1 << 4;

/// Feedback the renderer writes and the controller reads.
#[derive(Debug)]
pub(crate) struct Shared {
    pub position: AtomicI64,
    pub flags: AtomicU32,
    /// Id of the current song, 0 for none.
    pub song: AtomicU64,
    /// Peak per output channel as `f32` bits. Non-negative floats order like
    /// their bits, so `fetch_max` keeps the peak.
    pub meters: Vec<AtomicU32>,
}

/// Make a connected controller and renderer. Move the renderer to the audio
/// callback; keep the controller on the UI side.
#[must_use]
pub fn engine(config: EngineConfig) -> (Controller, Renderer) {
    let capacity = config.queue_capacity.max(1);
    let (commands, command_rx) = sync_channel(capacity);
    // A command retires at most two songs; the controller empties this
    // channel before every send, so it cannot fill up.
    let (retired_tx, retired) = sync_channel(capacity * 2 + 4);
    let (event_tx, events) = sync_channel(256);
    let shared = Arc::new(Shared {
        position: AtomicI64::new(0),
        flags: AtomicU32::new(0),
        song: AtomicU64::new(0),
        meters: (0..MAX_METER_CHANNELS).map(|_| AtomicU32::new(0)).collect(),
    });
    let renderer = Renderer::new(
        config,
        command_rx,
        retired_tx,
        event_tx,
        Arc::clone(&shared),
    );
    let controller = Controller {
        rate: config.sample_rate,
        commands,
        retired,
        events,
        shared,
        next_id: 1,
        song: None,
    };
    (controller, renderer)
}

#[derive(Debug)]
struct SongInfo {
    handle: SongHandle,
    tracks: usize,
    markers: Vec<MarkerSetup>,
}

/// The UI side of the engine. Every method returns at once.
#[derive(Debug)]
pub struct Controller {
    rate: u32,
    commands: SyncSender<Command>,
    retired: Receiver<Box<Program>>,
    events: Receiver<Event>,
    shared: Arc<Shared>,
    next_id: u64,
    song: Option<SongInfo>,
}

impl Controller {
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Cue a song, stopped at its start. A song that is playing stops with a
    /// short fade.
    ///
    /// # Errors
    /// [`EngineError::QueueFull`] or [`EngineError::Disconnected`].
    pub fn load(&mut self, song: SongSetup) -> Result<SongHandle, EngineError> {
        self.send_song(song, None)
    }

    /// Start `song` from its beginning while the current one fades out, both
    /// over `ms` milliseconds.
    ///
    /// # Errors
    /// [`EngineError::QueueFull`] or [`EngineError::Disconnected`].
    pub fn crossfade_to(&mut self, song: SongSetup, ms: f32) -> Result<SongHandle, EngineError> {
        self.send_song(song, Some(ms_to_frames(ms, self.rate)))
    }

    fn send_song(
        &mut self,
        setup: SongSetup,
        crossfade: Option<u32>,
    ) -> Result<SongHandle, EngineError> {
        self.collect();
        let handle = SongHandle(self.next_id);
        let info = SongInfo {
            handle,
            tracks: setup.tracks.len(),
            markers: setup.markers.clone(),
        };
        let program = Box::new(Program::new(handle.0, setup, self.rate));
        let cmd = match crossfade {
            None => Command::Load(program),
            Some(frames) => Command::Crossfade(program, frames),
        };
        self.send(cmd)?;
        self.next_id += 1;
        self.song = Some(info);
        Ok(handle)
    }

    /// Play from the current position. With `count_in`, the click counts the
    /// song's count-in bars first (when it has a tempo and count-in bars).
    ///
    /// # Errors
    /// No song, or the queue is full.
    pub fn play(&mut self, count_in: bool) -> Result<(), EngineError> {
        self.need_song()?;
        self.send(Command::Play { count_in })
    }

    /// Pause with a short fade; `play` continues from here.
    ///
    /// # Errors
    /// The queue is full.
    pub fn pause(&mut self) -> Result<(), EngineError> {
        self.send(Command::Pause)
    }

    /// Stop with a short fade and rewind to the start.
    ///
    /// # Errors
    /// The queue is full.
    pub fn stop(&mut self) -> Result<(), EngineError> {
        self.send(Command::Stop)
    }

    /// Move to `secs` into the tracks (with a short fade while playing).
    ///
    /// # Errors
    /// No song, or the queue is full.
    pub fn seek(&mut self, secs: f64) -> Result<(), EngineError> {
        self.need_song()?;
        let to = self.frames(secs);
        self.send(Command::Seek(to))
    }

    /// Fade in over `ms`, starting playback if it is stopped (no count-in).
    ///
    /// # Errors
    /// No song, or the queue is full.
    pub fn fade_in(&mut self, ms: f32) -> Result<(), EngineError> {
        self.need_song()?;
        self.send(Command::FadeIn(ms_to_frames(ms, self.rate)))
    }

    /// Fade out over `ms`, then stop and rewind.
    ///
    /// # Errors
    /// The queue is full.
    pub fn fade_out(&mut self, ms: f32) -> Result<(), EngineError> {
        self.send(Command::FadeOut(ms_to_frames(ms, self.rate)))
    }

    /// Loop the region of marker `name` (which needs an end), or stop
    /// looping with `None`. The loop wraps sample-exactly at the region's end.
    ///
    /// # Errors
    /// No song, an unknown marker, a marker without an end, or a full queue.
    pub fn loop_region(&mut self, name: Option<&str>) -> Result<(), EngineError> {
        let region = match name {
            None => None,
            Some(name) => {
                let m = self.marker(name)?;
                let end = m
                    .end_secs
                    .ok_or_else(|| EngineError::NotARegion(name.to_string()))?;
                let (start, end) = (self.frames(m.start_secs), self.frames(end));
                if end <= start {
                    return Err(EngineError::NotARegion(name.to_string()));
                }
                Some((start, end))
            }
        };
        self.send(Command::Loop(region))
    }

    /// Jump to marker `name`, now (with a short fade) or on the next bar line.
    ///
    /// # Errors
    /// No song, an unknown marker, or a full queue.
    pub fn jump_to_marker(&mut self, name: &str, at: JumpAt) -> Result<(), EngineError> {
        let to = self.frames(self.marker(name)?.start_secs);
        self.send(Command::Jump { to, at })
    }

    /// Names of the current song's markers, in order.
    #[must_use]
    pub fn markers(&self) -> Vec<&str> {
        self.song
            .iter()
            .flat_map(|s| s.markers.iter().map(|m| m.name.as_str()))
            .collect()
    }

    /// # Errors
    /// No such track, or a full queue.
    pub fn set_track_gain_db(&mut self, track: usize, db: f32) -> Result<(), EngineError> {
        self.track(track, TrackParam::GainDb(db))
    }

    /// # Errors
    /// No such track, or a full queue.
    pub fn set_track_pan(&mut self, track: usize, pan: f32) -> Result<(), EngineError> {
        self.track(track, TrackParam::Pan(pan))
    }

    /// # Errors
    /// No such track, or a full queue.
    pub fn set_track_mute(&mut self, track: usize, mute: bool) -> Result<(), EngineError> {
        self.track(track, TrackParam::Mute(mute))
    }

    /// While any track is soloed, only soloed tracks sound.
    ///
    /// # Errors
    /// No such track, or a full queue.
    pub fn set_track_solo(&mut self, track: usize, solo: bool) -> Result<(), EngineError> {
        self.track(track, TrackParam::Solo(solo))
    }

    /// # Errors
    /// No such track, or a full queue.
    pub fn set_track_bus(&mut self, track: usize, bus: usize) -> Result<(), EngineError> {
        self.track(track, TrackParam::Bus(bus))
    }

    /// Click during the song on or off (the count-in clicks either way).
    ///
    /// # Errors
    /// The queue is full.
    pub fn set_click_enabled(&mut self, on: bool) -> Result<(), EngineError> {
        self.send(Command::Click(ClickParam::Enabled(on)))
    }

    /// # Errors
    /// The queue is full.
    pub fn set_click_gain_db(&mut self, db: f32) -> Result<(), EngineError> {
        self.send(Command::Click(ClickParam::GainDb(db)))
    }

    /// # Errors
    /// The queue is full.
    pub fn set_click_bus(&mut self, bus: usize) -> Result<(), EngineError> {
        self.send(Command::Click(ClickParam::Bus(bus)))
    }

    /// The renderer's latest state.
    #[must_use]
    pub fn status(&self) -> Status {
        let flags = self.shared.flags.load(Ordering::Relaxed);
        let position_frames = self.shared.position.load(Ordering::Relaxed);
        let song = self.shared.song.load(Ordering::Relaxed);
        Status {
            song: (song != 0).then_some(SongHandle(song)),
            state: match flags & STATE_MASK {
                1 => PlayState::Playing,
                2 => PlayState::Paused,
                _ => PlayState::Stopped,
            },
            position_frames,
            position_secs: position_frames as f64 / f64::from(self.rate.max(1)),
            counting_in: flags & COUNTING_IN != 0,
            looping: flags & LOOPING != 0,
            unrouted: flags & UNROUTED != 0,
        }
    }

    /// Peak level per output channel (linear, 1.0 = full scale) since the
    /// last call.
    #[must_use]
    pub fn take_meters(&self) -> Vec<f32> {
        self.shared
            .meters
            .iter()
            .map(|m| f32::from_bits(m.swap(0, Ordering::Relaxed)))
            .collect()
    }

    /// Events since the last call, oldest first.
    pub fn events(&mut self) -> Vec<Event> {
        self.collect();
        self.events.try_iter().collect()
    }

    /// Free songs the renderer has let go of. Every method does this; call it
    /// from a UI timer when nothing else is sent for a long time.
    pub fn collect(&mut self) {
        for song in self.retired.try_iter() {
            drop(song);
        }
    }

    fn send(&mut self, cmd: Command) -> Result<(), EngineError> {
        self.collect();
        self.commands.try_send(cmd).map_err(|e| match e {
            TrySendError::Full(_) => EngineError::QueueFull,
            TrySendError::Disconnected(_) => EngineError::Disconnected,
        })
    }

    fn need_song(&self) -> Result<&SongInfo, EngineError> {
        self.song.as_ref().ok_or(EngineError::NoSong)
    }

    fn marker(&self, name: &str) -> Result<&MarkerSetup, EngineError> {
        self.need_song()?
            .markers
            .iter()
            .find(|m| m.name == name)
            .ok_or_else(|| EngineError::UnknownMarker(name.to_string()))
    }

    fn track(&mut self, track: usize, param: TrackParam) -> Result<(), EngineError> {
        if track >= self.need_song()?.tracks {
            return Err(EngineError::NoTrack(track));
        }
        self.send(Command::Track(track, param))
    }

    fn frames(&self, secs: f64) -> i64 {
        if secs.is_finite() {
            (secs.max(0.0) * f64::from(self.rate)).round() as i64
        } else {
            0
        }
    }

    /// The current song's handle, if one was loaded.
    #[must_use]
    pub fn song(&self) -> Option<SongHandle> {
        self.song.as_ref().map(|s| s.handle)
    }
}
