//! The GigCraft core for Swift (and later Kotlin), via UniFFI.
//!
//! A deliberately small surface: open or create a library, typed helpers for
//! the song list and charts, and `run` for every other command as JSON, so new
//! features rarely need new bindings (ADR-0009).

use gigcraft_commands::Engine;
use gigcraft_model::{SongId, format_duration};
use gigcraft_store::Library;
use std::sync::{Arc, Mutex, MutexGuard};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("{message}")]
    Failed { code: String, message: String },
}

impl CoreError {
    fn new(code: &str, message: impl ToString) -> Self {
        Self::Failed {
            code: code.to_string(),
            message: message.to_string(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct SongSummary {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub key: Option<String>,
    pub tempo_bpm: Option<f64>,
    pub duration: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ChartSegment {
    pub chord: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum ChartLine {
    Lyrics { segments: Vec<ChartSegment> },
    Comment { text: String },
    Empty,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct ChartBlock {
    /// plain, verse, chorus, bridge, tab, grid, chorus_recall, other
    pub kind: String,
    pub label: Option<String>,
    pub lines: Vec<ChartLine>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct Chart {
    pub key: Option<String>,
    pub blocks: Vec<ChartBlock>,
    pub warnings: Vec<String>,
}

/// An open library. Thread-safe; calls are serialised.
#[derive(Debug, uniffi::Object)]
pub struct Core {
    engine: Mutex<Engine>,
}

#[uniffi::export]
impl Core {
    /// Open the library in `path`.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<Arc<Self>, CoreError> {
        let lib = Library::open(path).map_err(|e| CoreError::new("storage", e))?;
        Ok(Arc::new(Self {
            engine: Mutex::new(Engine::new(lib)),
        }))
    }

    /// Open the library in `path`, or create it there with the demo content.
    #[uniffi::constructor]
    pub fn open_or_create_demo(path: String, name: String) -> Result<Arc<Self>, CoreError> {
        let dir = std::path::PathBuf::from(&path);
        if dir.join(gigcraft_store::MANIFEST).is_file() {
            return Self::open(path);
        }
        let lib = Library::create(&dir, &name).map_err(|e| CoreError::new("storage", e))?;
        let mut engine = Engine::new(lib);
        gigcraft_commands::demo::populate(&mut engine).map_err(|e| CoreError::new(e.code(), e))?;
        Ok(Arc::new(Self {
            engine: Mutex::new(engine),
        }))
    }

    /// Open the library in `path`, or create an empty one there.
    #[uniffi::constructor]
    pub fn open_or_create(path: String, name: String) -> Result<Arc<Self>, CoreError> {
        let dir = std::path::PathBuf::from(&path);
        if dir.join(gigcraft_store::MANIFEST).is_file() {
            return Self::open(path);
        }
        let lib = Library::create(&dir, &name).map_err(|e| CoreError::new("storage", e))?;
        Ok(Arc::new(Self {
            engine: Mutex::new(Engine::new(lib)),
        }))
    }

    /// Folder of the open library.
    pub fn library_path(&self) -> String {
        self.lock().library().root().display().to_string()
    }

    pub fn library_name(&self) -> String {
        self.lock().library().info().name.clone()
    }

    /// Songs matching `query` (empty: all), sorted by title.
    pub fn songs(&self, query: String) -> Vec<SongSummary> {
        let engine = self.lock();
        engine
            .library()
            .search(&query)
            .into_iter()
            .map(|s| SongSummary {
                id: s.id.to_string(),
                title: s.title.clone(),
                artist: s.artist.clone(),
                key: s.key.clone(),
                tempo_bpm: s.tempo_bpm,
                duration: s.duration_secs.map(format_duration),
            })
            .collect()
    }

    /// The chart of a song, transposed by `semitones`.
    pub fn chart(&self, song_id: String, semitones: i8) -> Result<Chart, CoreError> {
        let id: SongId = song_id
            .parse()
            .map_err(|_| CoreError::new("invalid_params", "bad song id"))?;
        let engine = self.lock();
        let text = engine
            .library()
            .chart(id)
            .ok_or_else(|| CoreError::new("not_found", "no such song"))?;
        let parsed = gigcraft_chordpro::parse(text, semitones);
        Ok(convert(parsed))
    }

    /// Run any command: `command` is its id, `params_json` a JSON object (or "").
    /// Returns the outcome as JSON: `{"result": …, "changes": […]}`.
    pub fn run(&self, command: String, params_json: String) -> Result<String, CoreError> {
        let params = if params_json.trim().is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(&params_json).map_err(|e| CoreError::new("invalid_params", e))?
        };
        let mut engine = self.lock();
        let out = engine
            .run(&command, &params)
            .map_err(|e| CoreError::new(e.code(), e))?;
        serde_json::to_string(&out).map_err(|e| CoreError::new("internal", e))
    }
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, Engine> {
        // A panic while holding the lock cannot leave the library half-written
        // (saves are atomic), so recovering the guard is safe.
        self.engine
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Parse a ChordPro text that is not saved yet (editor preview).
#[uniffi::export]
pub fn preview_chart(source: String) -> Chart {
    convert(gigcraft_chordpro::parse(&source, 0))
}

/// Semitones to transpose a chart written in `from` so it sounds in `to` (e.g. "G" → "A" = 2).
#[uniffi::export]
pub fn semitones_between(from: String, to: String) -> Option<i8> {
    gigcraft_chordpro::semitones_between(&from, &to)
}

/// The rig (MIDI devices, pedals, MIDI Learn) for the iOS app. CoreMIDI stays
/// in Swift; this object only turns events into outputs. Every method returns
/// the outputs as JSON: `[{"type":"send","port":…,"bytes":[…]}, {"type":"notice",…},
/// {"type":"action","action":"next"}, {"type":"learned",…}, {"type":"config_changed"}]`.
#[derive(Debug, uniffi::Object)]
pub struct RigCore {
    rig: Mutex<gigcraft_rig::Rig>,
}

fn outs_json(outs: Vec<gigcraft_rig::RigOut>) -> String {
    serde_json::to_string(&outs).unwrap_or_else(|_| "[]".into())
}

#[uniffi::export]
impl RigCore {
    /// A rig from its saved JSON configuration; an empty or invalid one gives the demo rig (simulated pedal).
    #[uniffi::constructor]
    pub fn new(config_json: String) -> Arc<Self> {
        let cfg = serde_json::from_str::<gigcraft_rig::RigConfig>(&config_json)
            .unwrap_or_else(|_| gigcraft_rig::RigConfig::demo());
        Arc::new(Self {
            rig: Mutex::new(gigcraft_rig::Rig::new(cfg)),
        })
    }

    pub fn config_json(&self) -> String {
        serde_json::to_string(self.lock().config()).unwrap_or_default()
    }

    pub fn set_config_json(&self, config_json: String, now_ms: u64) -> Result<String, CoreError> {
        let cfg =
            serde_json::from_str(&config_json).map_err(|e| CoreError::new("invalid_params", e))?;
        Ok(outs_json(self.lock().set_config(cfg, now_ms)))
    }

    pub fn ports_changed(&self, outputs: Vec<String>, now_ms: u64) -> String {
        outs_json(self.lock().ports_changed(&outputs, now_ms))
    }

    /// `actions_json`: the `actions` array of a `send_part` perform effect.
    pub fn send_part(&self, actions_json: String, now_ms: u64) -> String {
        let actions: Vec<gigcraft_model::MidiAction> =
            serde_json::from_str(&actions_json).unwrap_or_default();
        outs_json(self.lock().send_part(&actions, now_ms))
    }

    pub fn on_input(&self, port: String, bytes: Vec<u8>, now_ms: u64) -> String {
        outs_json(self.lock().on_input(&port, &bytes, now_ms))
    }

    pub fn tick(&self, now_ms: u64) -> String {
        outs_json(self.lock().tick(now_ms))
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.lock().next_deadline()
    }

    pub fn panic(&self, now_ms: u64) -> String {
        outs_json(self.lock().panic(now_ms))
    }

    pub fn learn(&self, on: bool) {
        self.lock().learn(on);
    }

    /// Map a learned trigger (JSON from a `learned` output) to an action ("next", "previous"…).
    pub fn add_mapping(&self, trigger_json: String, action: String) -> Result<String, CoreError> {
        let trigger =
            serde_json::from_str(&trigger_json).map_err(|e| CoreError::new("invalid_params", e))?;
        let action = serde_json::from_value(serde_json::Value::String(action))
            .map_err(|e| CoreError::new("invalid_params", e))?;
        Ok(outs_json(self.lock().add_mapping(gigcraft_rig::Mapping {
            trigger,
            action,
            port: None,
        })))
    }

    pub fn status_json(&self) -> String {
        serde_json::to_string(&self.lock().status()).unwrap_or_else(|_| "[]".into())
    }
}

impl RigCore {
    fn lock(&self) -> MutexGuard<'_, gigcraft_rig::Rig> {
        self.rig
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn convert(parsed: gigcraft_chordpro::Parsed) -> Chart {
    use gigcraft_chordpro::{BlockKind, ChartLine as L};
    let blocks = parsed
        .chart
        .blocks
        .into_iter()
        .map(|b| ChartBlock {
            kind: match b.kind {
                BlockKind::Plain => "plain",
                BlockKind::Verse => "verse",
                BlockKind::Chorus => "chorus",
                BlockKind::Bridge => "bridge",
                BlockKind::Tab => "tab",
                BlockKind::Grid => "grid",
                BlockKind::ChorusRecall => "chorus_recall",
                BlockKind::Other => "other",
            }
            .to_string(),
            label: b.label,
            lines: b
                .lines
                .into_iter()
                .map(|l| match l {
                    L::Lyrics { segments } => ChartLine::Lyrics {
                        segments: segments
                            .into_iter()
                            .map(|s| ChartSegment {
                                chord: s.chord,
                                text: s.text,
                            })
                            .collect(),
                    },
                    L::Comment { text, .. } => ChartLine::Comment { text },
                    L::Empty => ChartLine::Empty,
                })
                .collect(),
        })
        .collect();
    Chart {
        key: parsed.chart.key,
        blocks,
        warnings: parsed.warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_round_trip_through_the_ffi_surface() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lib").display().to_string();
        let core = Core::open_or_create_demo(path.clone(), "Demo".into()).unwrap();
        let songs = core.songs(String::new());
        assert_eq!(songs.len(), 5);
        let chart = core.chart(songs[0].id.clone(), 2).unwrap();
        assert!(!chart.blocks.is_empty());
        let out = core
            .run("song.list".into(), r#"{"query":"waltz"}"#.into())
            .unwrap();
        assert!(out.contains("Lantern Waltz"));
        assert!(core.run("nope".into(), String::new()).is_err());
        // Opening again does not add the demo twice.
        let again = Core::open_or_create_demo(path, "Demo".into()).unwrap();
        assert_eq!(again.songs(String::new()).len(), 5);
    }
}
