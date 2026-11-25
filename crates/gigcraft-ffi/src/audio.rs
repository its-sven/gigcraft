//! Backing tracks and click for the iOS app (ADR-0012, ADR-0014).
//!
//! Loading and decoding happen here, through UniFFI, on a thread the app
//! chooses (never the audio thread). The audio thread only renders, through
//! the C functions in `realtime`.

use crate::{Core, CoreError};
use gigcraft_audio::{
    Clip, Controller, EngineConfig, LoadError, PlayState, Renderer, SongSetup, engine,
};
use gigcraft_model::SongId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// What the audio engine is doing.
#[derive(Debug, Clone, uniffi::Record)]
pub struct AudioStatus {
    pub playing: bool,
    pub counting_in: bool,
    pub position_secs: f64,
    /// Length of the loaded song's longest track (0 for click only).
    pub length_secs: f64,
    /// The loaded song.
    pub song_id: Option<String>,
}

#[derive(Debug, uniffi::Object)]
pub struct AudioCore {
    controller: Mutex<Controller>,
    renderer: Mutex<Option<Renderer>>,
    clips: Mutex<HashMap<PathBuf, Clip>>,
    loaded: Mutex<Option<(String, f64)>>,
    rate: u32,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn parse_id(id: &str) -> Result<SongId, CoreError> {
    id.parse()
        .map_err(|_| CoreError::new("invalid_params", format!("not a song id: {id}")))
}

#[uniffi::export]
impl AudioCore {
    /// An engine for the output's sample rate; `max_frames` is the largest
    /// buffer the audio callback asks for.
    #[uniffi::constructor]
    pub fn new(sample_rate: u32, max_frames: u32) -> Arc<Self> {
        let mut config = EngineConfig::new(sample_rate.max(8000));
        config.max_block_frames = (max_frames as usize).max(256);
        let (controller, renderer) = engine(config);
        Arc::new(Self {
            controller: Mutex::new(controller),
            renderer: Mutex::new(Some(renderer)),
            clips: Mutex::new(HashMap::new()),
            loaded: Mutex::new(None),
            rate: sample_rate.max(8000),
        })
    }

    /// Hand the renderer to the audio thread (once). The handle is for
    /// `gigcraft_audio_render` and must be freed with
    /// `gigcraft_audio_free_renderer`; 0 if it was already taken.
    pub fn take_renderer(&self) -> u64 {
        lock(&self.renderer)
            .take()
            .map_or(0, crate::realtime::into_handle)
    }

    /// Does this song play anything (tracks, or a click with a tempo)?
    pub fn has_audio(&self, core: Arc<Core>, song_id: String) -> bool {
        let Ok(id) = parse_id(&song_id) else {
            return false;
        };
        let engine = core.lock();
        engine.library().song(id).is_some_and(|s| {
            !s.tracks.is_empty()
                || (s.click.as_ref().is_some_and(|c| c.enabled) && s.tempo_bpm.is_some())
        })
    }

    /// Decode the song's audio files into memory (call off the main thread).
    pub fn preload(&self, core: Arc<Core>, song_id: String) -> Result<(), CoreError> {
        let id = parse_id(&song_id)?;
        let paths: Vec<PathBuf> = {
            let engine = core.lock();
            let lib = engine.library();
            let Some(song) = lib.song(id) else {
                return Ok(());
            };
            song.tracks
                .iter()
                .filter_map(|t| lib.attachment_path(id, &t.file))
                .collect()
        };
        for path in paths {
            if lock(&self.clips).contains_key(&path) {
                continue;
            }
            let bytes = std::fs::read(&path)
                .map_err(|e| CoreError::new("storage", format!("{}: {e}", path.display())))?;
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase);
            let clip = Clip::decode(&bytes, ext.as_deref(), self.rate)
                .map_err(|e| CoreError::new("invalid_audio", format!("{}: {e}", path.display())))?;
            lock(&self.clips).insert(path, clip);
        }
        Ok(())
    }

    /// Load the song, stopped and ready to play (decodes what is missing).
    /// Returns the length in seconds.
    pub fn load_song(&self, core: Arc<Core>, song_id: String) -> Result<f64, CoreError> {
        if lock(&self.loaded)
            .as_ref()
            .is_some_and(|(s, _)| *s == song_id)
        {
            return Ok(lock(&self.loaded).as_ref().map_or(0.0, |(_, l)| *l));
        }
        self.preload(core.clone(), song_id.clone())?;
        let id = parse_id(&song_id)?;
        let engine = core.lock();
        let lib = engine.library();
        let song = lib
            .song(id)
            .ok_or_else(|| CoreError::new("not_found", format!("no song with id {song_id}")))?;
        let clips = lock(&self.clips);
        let setup = SongSetup::from_song(song, |t| {
            let path = lib.attachment_path(id, &t.file).ok_or(LoadError::Empty)?;
            clips.get(&path).cloned().ok_or(LoadError::Empty)
        })
        .map_err(|e| CoreError::new("invalid_audio", e))?;
        let length = setup
            .tracks
            .iter()
            .map(|t| t.clip.duration_secs())
            .fold(0.0, f64::max);
        lock(&self.controller)
            .load(setup)
            .map_err(|e| CoreError::new("audio", e))?;
        *lock(&self.loaded) = Some((song_id, length));
        Ok(length)
    }

    pub fn play(&self, count_in: bool) -> Result<(), CoreError> {
        lock(&self.controller)
            .play(count_in)
            .map_err(|e| CoreError::new("audio", e))
    }

    /// Stop with a short fade.
    pub fn stop(&self) -> Result<(), CoreError> {
        let mut c = lock(&self.controller);
        if c.status().state == PlayState::Stopped {
            return Ok(());
        }
        c.stop().map_err(|e| CoreError::new("audio", e))
    }

    /// Play (with the song's count-in) or stop.
    pub fn toggle(&self, core: Arc<Core>) -> Result<(), CoreError> {
        let playing = lock(&self.controller).status().state == PlayState::Playing;
        if playing {
            return self.stop();
        }
        let count_in = lock(&self.loaded)
            .as_ref()
            .and_then(|(s, _)| s.parse::<SongId>().ok())
            .and_then(|id| {
                let engine = core.lock();
                engine
                    .library()
                    .song(id)
                    .and_then(|s| s.click.as_ref())
                    .map(|c| c.enabled && c.count_in_bars > 0)
            })
            .unwrap_or(false);
        self.play(count_in)
    }

    /// Forget the loaded song (leaving perform mode); keeps decoded files.
    pub fn unload(&self) {
        let _ = self.stop();
        *lock(&self.loaded) = None;
    }

    pub fn status(&self) -> AudioStatus {
        let mut c = lock(&self.controller);
        let _ = c.events();
        let st = c.status();
        let loaded = lock(&self.loaded).clone();
        AudioStatus {
            playing: st.state == PlayState::Playing,
            counting_in: st.counting_in,
            position_secs: st.position_secs,
            length_secs: loaded.as_ref().map_or(0.0, |(_, l)| *l),
            song_id: loaded.map(|(s, _)| s),
        }
    }
}
