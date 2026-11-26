//! Backing tracks and click in perform mode, and the audio output settings.
//!
//! The engine (`gigcraft-audio`) runs in the platform's audio callback; the
//! shell provides the device through [`AudioHost`]. Audio files are decoded on
//! a worker thread and kept in memory, so changing songs on stage never waits
//! for the disk; the next song is decoded while the current one plays.

use crate::GigCraftApp;
use gigcraft_audio::{Clip, Event, LoadError, PlayState, SongSetup};
use gigcraft_model::{Song, SongId};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

/// An output device as the system names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    pub name: String,
    pub channels: u16,
    pub is_default: bool,
}

/// The audio output, provided by the shell (desktop: cpal).
pub trait AudioHost {
    fn devices(&mut self) -> Vec<AudioDevice>;
    /// Open `device` (or the system default) with `channels` (or its default);
    /// returns a description like "MacBook Speakers, 2 channels, 48000 Hz".
    fn open(&mut self, device: Option<&str>, channels: Option<u16>) -> Result<String, String>;
    fn close(&mut self);
    /// The engine's controller while a device is open.
    fn controller(&mut self) -> Option<&mut gigcraft_audio::Controller>;
    /// Problems reported by the device since the last call.
    fn errors(&mut self) -> Vec<String>;
}

/// Audio state of the app.
#[derive(Debug)]
pub(crate) struct AudioUi {
    /// Chosen device name (None: system default) and channel count.
    pub(crate) device: Option<String>,
    pub(crate) channels: Option<u16>,
    /// Description of the open device, or why it failed.
    pub(crate) status: String,
    opened: bool,
    devices: Vec<AudioDevice>,
    /// The song loaded into the engine (and a fingerprint of its audio settings).
    loaded: Option<(SongId, String)>,
    /// Length of the loaded song's longest track.
    length_secs: f64,
    /// Waiting for these files before the song can be loaded.
    waiting: HashSet<PathBuf>,
    clips: HashMap<PathBuf, Clip>,
    failed: HashMap<PathBuf, String>,
    decoded_tx: Sender<(PathBuf, Result<Clip, String>)>,
    decoded_rx: Receiver<(PathBuf, Result<Clip, String>)>,
    /// Test click running (rig window).
    testing: bool,
    /// Press Play once the next song is loaded (`--play`).
    pub(crate) autoplay: bool,
}

impl Default for AudioUi {
    fn default() -> Self {
        let (decoded_tx, decoded_rx) = channel();
        Self {
            device: None,
            channels: None,
            status: String::new(),
            opened: false,
            devices: Vec::new(),
            loaded: None,
            length_secs: 0.0,
            waiting: HashSet::new(),
            clips: HashMap::new(),
            failed: HashMap::new(),
            decoded_tx,
            decoded_rx,
            testing: false,
            autoplay: false,
        }
    }
}

/// Does this song play anything (tracks or a click)?
pub(crate) fn has_audio(song: &Song) -> bool {
    !song.tracks.is_empty()
        || (song.click.as_ref().is_some_and(|c| c.enabled) && song.tempo_bpm.is_some())
}

fn fingerprint(song: &Song) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        song.tracks, song.click, song.tempo_bpm, song.time_signature
    )
}

fn ensure_open(app: &mut GigCraftApp) -> bool {
    if app.audio.opened {
        return true;
    }
    let (device, channels) = (app.audio.device.clone(), app.audio.channels);
    let Some(host) = app.hooks.audio.as_mut() else {
        return false;
    };
    match host.open(device.as_deref(), channels) {
        Ok(desc) => {
            app.audio.status = desc;
            app.audio.opened = true;
            true
        }
        Err(e) => {
            app.audio.status = e.clone();
            app.message = Some((tr!("Audio output: {error}", error = e), true));
            false
        }
    }
}

fn sample_rate(app: &mut GigCraftApp) -> Option<u32> {
    app.hooks
        .audio
        .as_mut()
        .and_then(|h| h.controller())
        .map(|c| c.sample_rate())
}

/// Start decoding the song's audio files that are not in memory yet.
fn request_clips(app: &mut GigCraftApp, id: SongId) -> Vec<PathBuf> {
    let Some(rate) = sample_rate(app) else {
        return Vec::new();
    };
    let Some(engine) = app.engine.as_ref() else {
        return Vec::new();
    };
    let lib = engine.library();
    let Some(song) = lib.song(id) else {
        return Vec::new();
    };
    let mut missing = Vec::new();
    for t in &song.tracks {
        let Some(path) = lib.attachment_path(id, &t.file) else {
            continue;
        };
        if app.audio.clips.contains_key(&path) || app.audio.failed.contains_key(&path) {
            continue;
        }
        missing.push(path.clone());
        if app.audio.waiting.insert(path.clone()) {
            let tx = app.audio.decoded_tx.clone();
            let _ = std::thread::Builder::new()
                .name("gigcraft-decode".into())
                .spawn(move || {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(str::to_lowercase);
                    let result =
                        std::fs::read(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|bytes| {
                                Clip::decode(&bytes, ext.as_deref(), rate)
                                    .map_err(|e| e.to_string())
                            });
                    let _ = tx.send((path, result));
                });
        }
    }
    missing
}

/// Load `id` into the engine once its files are decoded (stopped, ready to play).
fn load(app: &mut GigCraftApp, id: SongId) {
    let Some(song) = app
        .engine
        .as_ref()
        .and_then(|e| e.library().song(id).cloned())
    else {
        return;
    };
    let key = (id, fingerprint(&song));
    if app.audio.loaded.as_ref() == Some(&key) {
        return;
    }
    if !has_audio(&song) || !ensure_open(app) {
        return;
    }
    if !request_clips(app, id).is_empty() {
        return; // try again when the files are decoded
    }
    let Some(lib) = app.engine.as_ref().map(|e| e.library()) else {
        return;
    };
    let (clips, failed) = (&app.audio.clips, &app.audio.failed);
    let setup = SongSetup::from_song(&song, |t| {
        let path = lib.attachment_path(id, &t.file).ok_or(LoadError::Empty)?;
        if failed.contains_key(&path) {
            return Err(LoadError::Empty);
        }
        clips.get(&path).cloned().ok_or(LoadError::Empty)
    });
    match setup {
        Ok(setup) => {
            let length = setup
                .tracks
                .iter()
                .map(|t| t.clip.duration_secs())
                .fold(0.0, f64::max);
            if let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller()) {
                match c.load(setup) {
                    Ok(_) => {
                        app.audio.loaded = Some(key);
                        app.audio.length_secs = length;
                        if std::mem::take(&mut app.audio.autoplay) {
                            toggle(app);
                        }
                    }
                    Err(e) => app.message = Some((e.to_string(), true)),
                }
            }
        }
        Err(e) => {
            app.audio.loaded = Some(key);
            app.message = Some((tr!("Backing track: {error}", error = e), true));
        }
    }
}

/// Every frame: decoded files in, device problems out, songs follow the show.
pub(crate) fn pump(app: &mut GigCraftApp, ctx: &egui::Context) {
    if app.hooks.audio.is_none() {
        return;
    }
    let mut arrived = false;
    while let Ok((path, result)) = app.audio.decoded_rx.try_recv() {
        app.audio.waiting.remove(&path);
        match result {
            Ok(clip) => {
                app.audio.clips.insert(path, clip);
            }
            Err(e) => {
                app.message = Some((
                    tr!(
                        "Could not read {file}: {error}",
                        file = path
                            .file_name()
                            .map(|f| f.to_string_lossy())
                            .unwrap_or_default(),
                        error = e
                    ),
                    true,
                ));
                app.audio.failed.insert(path, e);
            }
        }
        arrived = true;
    }
    let errors = app
        .hooks
        .audio
        .as_mut()
        .map(|h| h.errors())
        .unwrap_or_default();
    if let Some(e) = errors.last() {
        app.message = Some((tr!("Audio output: {error}", error = e), true));
    }
    // Drain engine events (song end, fades); the status shows the rest.
    if let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller()) {
        let _: Vec<Event> = c.events();
    }
    // Follow the show: load the current song, decode the next one.
    let songs = show_songs(app);
    if songs.is_none() && app.audio.loaded.is_some() {
        stop(app);
    }
    if let Some((current, next)) = songs {
        if let Some(id) = current {
            load(app, id);
        }
        if let Some(id) = next
            && app.audio.opened
        {
            request_clips(app, id);
        }
    }
    let playing = status(app).is_some_and(|s| s.state == PlayState::Playing);
    if playing || !app.audio.waiting.is_empty() || arrived {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// The current and next song of the running show.
fn show_songs(app: &GigCraftApp) -> Option<(Option<SongId>, Option<SongId>)> {
    let p = app.perform.as_ref()?;
    let snap = p.state();
    let song = |v: &serde_json::Value| v["song"].as_str().and_then(|s| s.parse().ok());
    // `next` in the perform state has no id; look it up in the running order.
    let current = song(&snap["current"]);
    let next = app
        .engine
        .as_ref()
        .and_then(|e| e.show())
        .and_then(|show| show.items.get(show.index + 1))
        .and_then(|item| match item {
            gigcraft_perform::Item::Song { song, .. } => Some(*song),
            _ => None,
        });
    Some((current, next))
}

fn status(app: &mut GigCraftApp) -> Option<gigcraft_audio::Status> {
    app.hooks
        .audio
        .as_mut()
        .and_then(|h| h.controller())
        .map(|c| c.status())
}

/// Stop what plays (with a short fade).
fn halt(app: &mut GigCraftApp) {
    if let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller())
        && c.status().state != PlayState::Stopped
    {
        let _ = c.stop();
    }
}

/// Play or stop the loaded song (keyboard P, a footswitch, a MIDI controller).
pub(crate) fn toggle(app: &mut GigCraftApp) {
    let Some((id, _)) = app.audio.loaded.clone() else {
        return;
    };
    let count_in = app
        .engine
        .as_ref()
        .and_then(|e| e.library().song(id))
        .and_then(|s| s.click.as_ref())
        .is_some_and(|c| c.enabled && c.count_in_bars > 0);
    if let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller()) {
        let _ = if c.status().state == PlayState::Playing {
            c.stop()
        } else {
            c.play(count_in)
        };
    }
}

/// Position of the playing song's tracks (not during the count-in).
pub(crate) fn position(app: &mut GigCraftApp) -> Option<f64> {
    status(app)
        .filter(|s| s.state == PlayState::Playing && !s.counting_in)
        .map(|s| s.position_secs)
}

/// PANIC: stop the audio too.
pub(crate) fn panic(app: &mut GigCraftApp) {
    halt(app);
}

/// Leaving perform mode: stop.
fn stop(app: &mut GigCraftApp) {
    halt(app);
    app.audio.loaded = None;
}

fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Play/stop and position for the current song, in the perform top bar.
pub(crate) fn transport(app: &mut GigCraftApp, ui: &mut egui::Ui, song: Option<SongId>) {
    if app.hooks.audio.is_none() {
        return;
    }
    let Some(id) = song else { return };
    let Some(s) = app
        .engine
        .as_ref()
        .and_then(|e| e.library().song(id).cloned())
    else {
        return;
    };
    if !has_audio(&s) {
        return;
    }
    let ready = app.audio.loaded.as_ref().is_some_and(|(l, _)| *l == id);
    if !ready {
        ui.label(egui::RichText::new(tr!("Loading backing tracks…")).weak());
        return;
    }
    let st = status(app);
    let playing = st.as_ref().is_some_and(|s| s.state == PlayState::Playing);
    let count_in = s.click.as_ref().map_or(0, |c| c.count_in_bars);
    let label = if playing { tr!("Stop") } else { tr!("Play") };
    let tip = if playing {
        tr!("Stop the backing tracks (short fade; key P)")
    } else if count_in > 0 {
        tr!("Start the backing tracks after the count-in (key P)")
    } else {
        tr!("Start the backing tracks and click (key P)")
    };
    let button = if playing {
        egui::Button::new(egui::RichText::new(label).strong())
    } else {
        crate::theme::primary(label)
    };
    if ui.add(button).on_hover_text(tip).clicked()
        && let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller())
    {
        let _ = if playing {
            c.stop()
        } else {
            c.play(count_in > 0)
        };
    }
    if let Some(st) = st {
        let text = if st.counting_in {
            tr!("count-in").to_string()
        } else if app.audio.length_secs > 0.0 {
            format!(
                "{} / {}",
                clock(st.position_secs),
                clock(app.audio.length_secs)
            )
        } else {
            clock(st.position_secs)
        };
        crate::theme::readout(ui, tr!("Track"), &text, 15.0);
    }
}

/// The audio part of the rig window.
pub(crate) fn settings(app: &mut GigCraftApp, ui: &mut egui::Ui) {
    crate::theme::section(ui, tr!("Audio output (backing tracks and click)"));
    if app.hooks.audio.is_none() {
        ui.label(tr!("No audio output on this platform."));
        return;
    }
    if app.audio.devices.is_empty()
        && let Some(h) = app.hooks.audio.as_mut()
    {
        app.audio.devices = h.devices();
    }
    let mut reopen = false;
    ui.horizontal(|ui| {
        ui.label(tr!("Device"));
        let current = app
            .audio
            .device
            .clone()
            .unwrap_or_else(|| tr!("System default").to_string());
        egui::ComboBox::from_id_salt("audio-device")
            .selected_text(current)
            .show_ui(ui, |ui| {
                reopen |= ui
                    .selectable_value(&mut app.audio.device, None, tr!("System default"))
                    .changed();
                for d in &app.audio.devices {
                    reopen |= ui
                        .selectable_value(&mut app.audio.device, Some(d.name.clone()), &d.name)
                        .on_hover_text(trn!(d.channels, "{n} output", "{n} outputs"))
                        .changed();
                }
            })
            .response
            .on_hover_text(tr!("Where backing tracks and the click play; an audio interface with more outputs can send the click to in-ears only"));
        if ui
            .small_button(tr!("Refresh"))
            .on_hover_text(tr!("Look for audio devices again"))
            .clicked()
        {
            app.audio.devices.clear();
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr!("Outputs"));
        let mut ch = app.audio.channels.unwrap_or(2);
        if ui
            .add(egui::DragValue::new(&mut ch).range(1..=32))
            .on_hover_text(tr!(
                "How many outputs to use; tracks pick a pair (1/2, 3/4…) in the song editor"
            ))
            .changed()
        {
            app.audio.channels = Some(ch);
            reopen = true;
        }
    });
    if reopen {
        app.audio.opened = false;
        app.audio.loaded = None;
        app.audio.clips.clear();
        if let Some(h) = app.hooks.audio.as_mut() {
            h.close();
        }
        ensure_open(app);
    }
    ui.horizontal(|ui| {
        if !app.audio.status.is_empty() {
            ui.small(app.audio.status.as_str());
        }
        let label = if app.audio.testing {
            tr!("Stop test")
        } else {
            tr!("Test click")
        };
        if ui
            .small_button(label)
            .on_hover_text(tr!("Play a click at 120 BPM to check the output"))
            .clicked()
        {
            test_click(app);
        }
    });
}

fn test_click(app: &mut GigCraftApp) {
    if !ensure_open(app) {
        return;
    }
    let testing = app.audio.testing;
    let Some(c) = app.hooks.audio.as_mut().and_then(|h| h.controller()) else {
        return;
    };
    if testing {
        let _ = c.stop();
        app.audio.testing = false;
        return;
    }
    let setup = SongSetup {
        tracks: Vec::new(),
        tempo: Some(gigcraft_audio::Tempo {
            bpm: 120.0,
            beats_per_bar: 4,
        }),
        click: gigcraft_audio::ClickSetup {
            enabled: true,
            ..Default::default()
        },
        markers: Vec::new(),
    };
    if c.load(setup).is_ok() && c.play(false).is_ok() {
        app.audio.testing = true;
        app.audio.loaded = None;
    }
}

pub(crate) fn load_settings(app: &mut GigCraftApp, storage: &dyn eframe::Storage) {
    app.audio.device = storage.get_string(DEVICE_KEY).filter(|d| !d.is_empty());
    app.audio.channels = storage
        .get_string(CHANNELS_KEY)
        .and_then(|c| c.parse().ok());
}

pub(crate) fn save_settings(app: &GigCraftApp, storage: &mut dyn eframe::Storage) {
    storage.set_string(DEVICE_KEY, app.audio.device.clone().unwrap_or_default());
    storage.set_string(
        CHANNELS_KEY,
        app.audio
            .channels
            .map(|c| c.to_string())
            .unwrap_or_default(),
    );
}

const DEVICE_KEY: &str = "gigcraft.audio_device";
const CHANNELS_KEY: &str = "gigcraft.audio_channels";
