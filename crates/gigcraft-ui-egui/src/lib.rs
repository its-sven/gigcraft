//! GigCraft's desktop and web interface in egui.
//!
//! Library browser and editors for songs and setlists, and perform mode for
//! the stage. Every change goes through `gigcraft-commands`, like the CLI and
//! MCP. Platform services (folder dialogs) are passed in as [`Hooks`], so this
//! crate also builds for the web.

#[macro_use]
pub mod i18n;
mod audio_ui;
mod chart_view;
mod help;
mod live_ui;
mod perform;
mod rig_ui;
mod setlist_view;
mod song_view;
mod stage_display;
mod theme;

use gigcraft_commands::{Engine, Outcome};
use gigcraft_model::{SetlistId, SongId};
use gigcraft_perform::Keymap;
use gigcraft_rig::{Rig, RigConfig};
use gigcraft_store::Library;

pub use audio_ui::{AudioDevice, AudioHost};
pub use live_ui::{HostEvent, Hosted, JoinEvent, LiveIo};
pub use rig_ui::MidiIo;
use serde_json::{Value, json};
use std::path::PathBuf;

const LAST_LIBRARY_KEY: &str = "gigcraft.last_library";
const FONT_SIZE_KEY: &str = "gigcraft.font_size";
const KEYMAP_KEY: &str = "gigcraft.keymap";
const RIG_KEY: &str = "gigcraft.rig";
const LANGUAGE_KEY: &str = "gigcraft.language";

/// Give a widget whose text is a symbol (− +) a name for screen readers.
pub(crate) fn named(r: egui::Response, name: &str) -> egui::Response {
    let enabled = r.enabled();
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    r
}

/// Platform services the shell can offer.
#[derive(Default)]
pub struct Hooks {
    /// Ask the user for a folder (open or create a library).
    pub pick_folder: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    /// Ask the user for a file (attachments, imports).
    pub pick_file: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    /// MIDI ports (desktop).
    pub midi: Option<Box<dyn MidiIo>>,
    /// Local network for live sessions (desktop).
    pub live: Option<Box<dyn LiveIo>>,
    /// Audio output for backing tracks and click (desktop).
    pub audio: Option<Box<dyn AudioHost>>,
}

impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks")
            .field("pick_folder", &self.pick_folder.is_some())
            .field("pick_file", &self.pick_file.is_some())
            .field("midi", &self.midi.is_some())
            .field("live", &self.live.is_some())
            .field("audio", &self.audio.is_some())
            .finish()
    }
}

/// What to show first (from the command line).
#[derive(Debug, Clone, Default)]
pub struct Start {
    pub library: Option<PathBuf>,
    /// Song title (first match) or id.
    pub song: Option<String>,
    /// Setlist name (first match) or id.
    pub setlist: Option<String>,
    /// Open the song in the editor.
    pub edit: bool,
    /// Start perform mode with the setlist.
    pub perform: bool,
    /// Open the rig window.
    pub rig: bool,
    /// Interface language for this run (`en`, `de`, `auto`); otherwise the saved setting.
    pub language: Option<String>,
    /// Start leading a live session.
    pub live: bool,
    /// Join a live session at this address.
    pub join: Option<String>,
    /// Open the help window.
    pub help: bool,
    /// Open the Live window (without starting or joining).
    pub show_live: bool,
    /// Open screen 2: "chart" or "lyrics".
    pub screen2: Option<String>,
    /// Press Play once the first song's backing tracks are loaded.
    pub play: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Songs,
    Setlists,
}

/// The application state.
#[derive(Debug)]
pub struct GigCraftApp {
    engine: Option<Engine>,
    hooks: Hooks,
    view: View,
    query: String,
    song: Option<SongId>,
    setlist: Option<SetlistId>,
    transpose: i8,
    font_size: f32,
    message: Option<(String, bool)>,
    chart_cache: chart_view::Cache,
    song_draft: Option<song_view::Draft>,
    setlist_ui: setlist_view::State,
    perform: Option<perform::PerformUi>,
    keymap: Keymap,
    /// Code of the last failed command (e.g. "conflict").
    last_error: Option<&'static str>,
    /// When the library folder was last checked for outside changes (egui time).
    last_disk_check: f64,
    /// Outside changes seen while editing (reload waits until the editor closes).
    pending_reload: bool,
    rig: Rig,
    rig_ui: rig_ui::RigUi,
    /// The rig configuration changed and must be saved.
    rig_dirty: bool,
    /// Text of the "Paste" window (ChordPro, chords over lyrics, Ultimate Guitar links), if open.
    paste: Option<String>,
    /// Interface language setting.
    language: i18n::Setting,
    live: live_ui::LiveUi,
    help: help::HelpUi,
    audio: audio_ui::AudioUi,
    stage: stage_display::StageUi,
}

impl GigCraftApp {
    /// Create the app; opens `start.library` or the last library used.
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start, hooks: Hooks) -> Self {
        theme::install(&cc.egui_ctx);
        let mut app = Self {
            engine: None,
            hooks,
            view: View::Songs,
            query: String::new(),
            song: None,
            setlist: None,
            transpose: 0,
            font_size: 20.0,
            message: None,
            chart_cache: chart_view::Cache::default(),
            song_draft: None,
            setlist_ui: setlist_view::State::default(),
            perform: None,
            keymap: Keymap::default(),
            last_error: None,
            last_disk_check: 0.0,
            pending_reload: false,
            rig: Rig::new(RigConfig::demo()),
            rig_ui: rig_ui::RigUi::default(),
            rig_dirty: false,
            paste: None,
            language: i18n::Setting::System,
            live: live_ui::LiveUi::default(),
            help: help::HelpUi::default(),
            audio: audio_ui::AudioUi::default(),
            stage: stage_display::StageUi::default(),
        };
        if let Some(storage) = cc.storage {
            if let Some(lang) = storage.get_string(LANGUAGE_KEY) {
                app.language = i18n::Setting::parse(&lang);
            }
            i18n::set(app.language.lang());
            live_ui::load(&mut app, storage);
            audio_ui::load_settings(&mut app, storage);
            if let Some(size) = storage
                .get_string(FONT_SIZE_KEY)
                .and_then(|s| s.parse().ok())
            {
                app.font_size = size;
            }
            if let Some(km) = storage
                .get_string(KEYMAP_KEY)
                .and_then(|s| serde_json::from_str(&s).ok())
            {
                app.keymap = km;
            }
            if let Some(cfg) = storage
                .get_string(RIG_KEY)
                .and_then(|s| serde_json::from_str::<RigConfig>(&s).ok())
            {
                app.rig = Rig::new(cfg);
            }
            let last = storage.get_string(LAST_LIBRARY_KEY).map(PathBuf::from);
            if start.library.is_none()
                && let Some(path) = last.filter(|p| p.is_dir())
            {
                app.open(path);
            }
        }
        if let Some(code) = &start.language {
            app.language = i18n::Setting::parse(code);
        }
        i18n::set(app.language.lang());
        if let Some(path) = start.library.clone() {
            app.open(path);
        }
        app.select(&start);
        app.start_live(&start);
        app.help.open = start.help;
        if start.show_live {
            app.live.open = true;
        }
        app.stage.mode = match start.screen2.as_deref() {
            Some("chart") => stage_display::Mode::Chart,
            Some("lyrics") => stage_display::Mode::Lyrics,
            _ => stage_display::Mode::Off,
        };
        app.audio.autoplay = start.play;
        app
    }

    fn start_live(&mut self, start: &Start) {
        if let Some(addr) = &start.join {
            self.live.address = addr.clone();
            live_ui::start_join(self);
        } else if start.live {
            live_ui::start_host(self);
            self.live.open = true;
        }
    }

    fn select(&mut self, start: &Start) {
        self.rig_ui.open = start.rig;
        let Some(engine) = &self.engine else { return };
        let lib = engine.library();
        let norm = gigcraft_store::normalize;
        if let Some(want) = &start.song {
            let w = norm(want);
            self.song = lib
                .songs()
                .into_iter()
                .find(|s| s.id.to_string() == *want || norm(&s.title).contains(&w))
                .map(|s| s.id);
            self.view = View::Songs;
        }
        if let Some(want) = &start.setlist {
            let w = norm(want);
            self.setlist = lib
                .setlists()
                .into_iter()
                .find(|s| s.id.to_string() == *want || norm(&s.name).contains(&w))
                .map(|s| s.id);
            self.view = View::Setlists;
        }
        if start.edit
            && let Some(id) = self.song
        {
            self.song_draft = song_view::Draft::load(self, id);
            self.view = View::Songs;
        }
        if start.edit && start.song.is_none() && self.setlist.is_some() {
            self.setlist_ui = setlist_view::State::editing();
        }
        if start.perform
            && let Some(id) = self.setlist
        {
            perform::start(self, id, None);
        }
    }

    fn open(&mut self, path: PathBuf) {
        match Library::open(&path) {
            Ok(lib) => {
                let problems = lib.problems().len();
                self.engine = Some(Engine::new(lib));
                self.song = None;
                self.setlist = None;
                self.song_draft = None;
                self.perform = None;
                self.message = (problems > 0).then(|| {
                    (
                        trn!(
                            problems,
                            "{n} file could not be read – see library.info",
                            "{n} files could not be read – see library.info"
                        ),
                        true,
                    )
                });
            }
            Err(e) => self.message = Some((e.to_string(), true)),
        }
    }

    fn create_demo(&mut self, path: PathBuf) {
        let target = path.join("GigCraft Demo");
        let result = Library::create(&target, "GigCraft Demo")
            .map(Engine::new)
            .and_then(|mut e| {
                gigcraft_commands::demo::populate(&mut e).map_err(|err| {
                    gigcraft_store::StoreError::Format {
                        path: target.clone(),
                        message: err.to_string(),
                    }
                })?;
                Ok(e)
            });
        match result {
            Ok(engine) => {
                self.engine = Some(engine);
                self.message = Some((
                    tr!("Demo library created in {path}", path = target.display()),
                    false,
                ));
            }
            Err(e) => self.message = Some((e.to_string(), true)),
        }
    }

    /// Run a command and report errors in the status line.
    pub(crate) fn run(&mut self, id: &str, params: Value) -> Option<Outcome> {
        let engine = self.engine.as_mut()?;
        match engine.run(id, &params) {
            Ok(out) => {
                self.last_error = None;
                Some(out)
            }
            Err(e) => {
                self.last_error = Some(e.code());
                self.message = Some((e.to_string(), true));
                None
            }
        }
    }

    /// Every few seconds: pick up changes made by another device or program
    /// (iCloud Drive, Nextcloud, Syncthing, the CLI). Not while performing;
    /// while editing, only after the editor is closed.
    fn watch_disk(&mut self, now: f64) {
        if self.engine.is_none() || self.perform.is_some() || now - self.last_disk_check < 3.0 {
            return;
        }
        self.last_disk_check = now;
        let changed = self
            .engine
            .as_mut()
            .and_then(|e| e.run("library.check", &Value::Null).ok())
            .and_then(|o| o.result["external_changes"].as_u64())
            .unwrap_or(0);
        if changed > 0 || self.pending_reload {
            if self.song_draft.is_some() || self.setlist_ui.editing {
                self.pending_reload = true;
                return;
            }
            self.pending_reload = false;
            if self.run("library.reload", Value::Null).is_some() {
                self.chart_cache = chart_view::Cache::default();
                self.notify(trn!(
                    changed,
                    "Updated from disk ({n} change from another device or program).",
                    "Updated from disk ({n} changes from another device or program)."
                ));
            }
        }
    }

    pub(crate) fn notify(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let mut want_demo = None;
            if self.hooks.pick_folder.is_some() {
                ui.menu_button(tr!("Library"), |ui| {
                    if ui.button(tr!("Open library…")).on_hover_text(tr!("Open a folder with GigCraft songs and setlists – also one shared through iCloud Drive, Nextcloud or Syncthing")).clicked() {
                        want_demo = Some(false);
                        ui.close();
                    }
                    if ui
                        .button(tr!("New demo library…"))
                        .on_hover_text(tr!(
                            "Creates a folder “GigCraft Demo” in the folder you pick"
                        ))
                        .clicked()
                    {
                        want_demo = Some(true);
                        ui.close();
                    }
                });
                ui.separator();
            }
            if let Some(demo) = want_demo
                && let Some(path) = self.hooks.pick_folder.as_ref().and_then(|pick| pick())
            {
                if demo {
                    self.create_demo(path);
                } else {
                    self.open(path);
                }
            }
            let (undo, redo) = self.engine.as_ref().map_or((None, None), |e| {
                (
                    e.undo_label().map(str::to_string),
                    e.redo_label().map(str::to_string),
                )
            });
            if ui
                .add_enabled(undo.is_some(), egui::Button::new(tr!("Undo")))
                .on_hover_text(undo.unwrap_or_default())
                .clicked()
            {
                self.run("edit.undo", Value::Null);
                self.song_draft = None;
            }
            if ui
                .add_enabled(redo.is_some(), egui::Button::new(tr!("Redo")))
                .on_hover_text(redo.unwrap_or_default())
                .clicked()
            {
                self.run("edit.redo", Value::Null);
                self.song_draft = None;
            }
            if ui
                .button(tr!("Live…"))
                .on_hover_text(tr!("Play together: the band follows your setlist"))
                .clicked()
            {
                self.live.open = !self.live.open;
            }
            if ui
                .button(tr!("Rig…"))
                .on_hover_text(tr!("MIDI devices, pedals, footswitches and the audio output"))
                .clicked()
            {
                self.rig_ui.open = !self.rig_ui.open;
            }
            if ui
                .button(tr!("Help"))
                .on_hover_text(tr!("How GigCraft works, and keyboard shortcuts (F1)"))
                .clicked()
            {
                self.help.open = !self.help.open;
            }
            ui.separator();
            ui.label(tr!("Text size"));
            ui.add(egui::Slider::new(&mut self.font_size, 12.0..=48.0).show_value(false))
                .widget_info(|| {
                    egui::WidgetInfo::slider(true, f64::from(self.font_size), tr!("Text size"))
                });
            self.language_picker(ui);
            if let Some(e) = &self.engine {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&e.library().info().name).strong());
                });
            }
        });
    }

    fn language_picker(&mut self, ui: &mut egui::Ui) {
        let current = match &self.language {
            i18n::Setting::System => tr!("System language").to_string(),
            i18n::Setting::Code(code) => i18n::languages()
                .into_iter()
                .find(|(c, _)| c == code)
                .map_or_else(|| code.clone(), |(_, name)| name.to_string()),
        };
        let mut pick = None;
        egui::ComboBox::from_id_salt("language")
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(
                        self.language == i18n::Setting::System,
                        tr!("System language"),
                    )
                    .clicked()
                {
                    pick = Some(i18n::Setting::System);
                }
                for (code, name) in i18n::languages() {
                    let setting = i18n::Setting::Code(code);
                    if ui
                        .selectable_label(self.language == setting, name)
                        .clicked()
                    {
                        pick = Some(setting);
                    }
                }
            })
            .response
            .on_hover_text(tr!("Language"))
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, tr!("Language")));
        if let Some(setting) = pick {
            i18n::set(setting.lang());
            self.language = setting;
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Songs, tr!("Songs"))
                .on_hover_text(tr!("All songs in the library"));
            ui.selectable_value(&mut self.view, View::Setlists, tr!("Setlists"))
                .on_hover_text(tr!("Running orders for gigs and rehearsals"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, cmd) = match self.view {
                    View::Songs => (tr!("New song"), "song"),
                    View::Setlists => (tr!("New setlist"), "setlist"),
                };
                if ui
                    .small_button(label)
                    .on_hover_text(tr!("Create one and open the editor"))
                    .clicked()
                {
                    self.create(cmd);
                }
            });
        });
        if self.view == View::Songs {
            ui.horizontal(|ui| {
                if self.hooks.pick_file.is_some()
                    && ui
                        .small_button(tr!("Import file…"))
                        .on_hover_text(tr!(
                            "ChordPro, OnSong, OpenSong, text, BandHelper TSV, CSV, Cuebook"
                        ))
                        .clicked()
                    && let Some(path) = self.hooks.pick_file.as_ref().and_then(|p| p())
                {
                    self.import_path(path);
                }
                if ui
                    .small_button(tr!("Paste…"))
                    .on_hover_text(tr!("Chords over lyrics, ChordPro or Ultimate Guitar links"))
                    .clicked()
                {
                    self.paste = Some(String::new());
                }
            });
        }
        ui.separator();
        let Some(engine) = &self.engine else { return };
        let mut pick_song = None;
        let mut pick_list = None;
        match self.view {
            View::Songs => {
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text(tr!("Search: words, \"phrase\", -not")),
                );
                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for song in engine.library().search(&self.query) {
                            let selected = self.song == Some(song.id);
                            let mut text = egui::RichText::new(&song.title);
                            if selected {
                                text = text.strong();
                            }
                            let r = ui.selectable_label(selected, text);
                            let r = if song.artist.is_empty() {
                                r
                            } else {
                                r.on_hover_text(&song.artist)
                            };
                            if r.clicked() {
                                pick_song = Some(song.id);
                            }
                        }
                    });
            }
            View::Setlists => {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        for list in engine.library().setlists() {
                            let label = match &list.date {
                                Some(d) => format!("{d}  {}", list.name),
                                None => list.name.clone(),
                            };
                            if ui
                                .selectable_label(self.setlist == Some(list.id), label)
                                .clicked()
                            {
                                pick_list = Some(list.id);
                            }
                        }
                    });
            }
        }
        if let Some(id) = pick_song {
            self.song = Some(id);
            self.transpose = 0;
            self.song_draft = None;
        }
        if let Some(id) = pick_list {
            self.setlist = Some(id);
            self.setlist_ui = setlist_view::State::default();
        }
    }

    fn create(&mut self, what: &str) {
        match what {
            "song" => {
                if let Some(out) = self.run("song.create", json!({
                    "title": tr!("New song"),
                    "chart": format!("{{title: {}}}\n\n{}\n", tr!("New song"), tr!("[C]Lyrics with [G]chords")),
                }))
                    && let Some(id) = out.result["id"].as_str().and_then(|s| s.parse().ok())
                {
                    self.song = Some(id);
                    self.view = View::Songs;
                    self.song_draft = song_view::Draft::load(self, id);
                }
            }
            _ => {
                if let Some(out) = self.run("setlist.create", json!({"name": tr!("New setlist")}))
                    && let Some(id) = out.result["id"].as_str().and_then(|s| s.parse().ok())
                {
                    self.setlist = Some(id);
                    self.view = View::Setlists;
                    self.setlist_ui = setlist_view::State::editing();
                }
            }
        }
    }

    fn import_path(&mut self, path: PathBuf) {
        if let Some(out) = self.run("import.file", json!({"path": path.display().to_string()})) {
            self.after_import(&out.result);
        }
    }

    fn after_import(&mut self, r: &Value) {
        let warnings = r["warnings"].as_array().map_or(0, Vec::len);
        let mut text = tr!(
            "Imported: {new} new and {updated} updated songs, {setlists} setlists.",
            new = r["songs_created"],
            updated = r["songs_updated"],
            setlists = r["setlists"]
        );
        if warnings > 0 {
            text.push(' ');
            text.push_str(&trn!(
                warnings,
                "{n} warning – see the songs' notes.",
                "{n} warnings – see the songs' notes."
            ));
        }
        self.notify(text);
        if let Some(id) = r["first_song"].as_str().and_then(|s| s.parse().ok()) {
            self.song = Some(id);
            self.view = View::Songs;
        }
    }

    fn paste_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.paste.take() else {
            return;
        };
        let mut open = true;
        let mut action = None;
        egui::Window::new(tr!("Paste songs"))
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(tr!(
                    "Paste a song (ChordPro or chords over lyrics) or Ultimate Guitar links."
                ));
                ui.add(
                    egui::TextEdit::multiline(&mut text)
                        .code_editor()
                        .desired_rows(14)
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr!("Import song")).on_hover_text(tr!("Add the pasted song to the library")).clicked() {
                        action = Some("import.text");
                    }
                    if ui.button(tr!("Add Ultimate Guitar links")).on_hover_text(tr!("Create a song for each link, with the link attached (charts are not copied)")).clicked() {
                        action = Some("import.ultimate_guitar");
                    }
                });
            });
        match action {
            Some("import.text") => {
                if let Some(out) = self.run("import.text", json!({"text": text})) {
                    self.after_import(&out.result);
                    return;
                }
            }
            Some(_) => {
                if let Some(out) = self.run("import.ultimate_guitar", json!({"links": text})) {
                    let n = out.result["created"].as_array().map_or(0, Vec::len);
                    let mut text = trn!(
                        n,
                        "{n} song added with its Ultimate Guitar link.",
                        "{n} songs added with their Ultimate Guitar links."
                    );
                    if let Some(skipped) =
                        out.result["skipped_existing"].as_u64().filter(|s| *s > 0)
                    {
                        text.push(' ');
                        text.push_str(&trn!(
                            skipped,
                            "{n} was already in the library.",
                            "{n} were already in the library."
                        ));
                    }
                    self.notify(text);
                    return;
                }
            }
            None => {}
        }
        if open {
            self.paste = Some(text);
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading(egui::RichText::new(tr!("Welcome to GigCraft")).size(28.0));
            ui.add_space(4.0);
            ui.label(tr!(
                "Setlists, chord charts and your pedal presets – on stage and with the band."
            ));
            ui.add_space(16.0);
            ui.label(tr!(
                "Open a library folder, or create the demo library to look around."
            ));
            ui.add_space(8.0);
            welcome_buttons(self, ui);
            if self.hooks.pick_folder.is_none() {
                ui.label(tr!("Start with: gigcraft <library folder>"));
            }
        });
    }
}

/// The first steps: demo library, own folder, help.
pub(crate) fn welcome_buttons(app: &mut GigCraftApp, ui: &mut egui::Ui) {
    let big = |t: &str| {
        egui::Button::new(egui::RichText::new(t).size(18.0)).min_size(egui::vec2(220.0, 40.0))
    };
    let mut want_demo = None;
    ui.horizontal(|ui| {
        if app.hooks.pick_folder.is_some() {
            if ui
                .add(big(tr!("New demo library…")))
                .on_hover_text(tr!("Creates a folder “GigCraft Demo” in the folder you pick"))
                .clicked()
            {
                want_demo = Some(true);
            }
            if ui
                .add(big(tr!("Open library…")))
                .on_hover_text(tr!("Open a folder with GigCraft songs and setlists – also one shared through iCloud Drive, Nextcloud or Syncthing"))
                .clicked()
            {
                want_demo = Some(false);
            }
        }
        if ui
            .add(big(tr!("Help")))
            .on_hover_text(tr!("How GigCraft works, and keyboard shortcuts (F1)"))
            .clicked()
        {
            app.help.show(help::Topic::Start);
        }
    });
    if let Some(demo) = want_demo
        && let Some(path) = app.hooks.pick_folder.as_ref().and_then(|pick| pick())
    {
        if demo {
            app.create_demo(path);
        } else {
            app.open(path);
        }
    }
}

impl eframe::App for GigCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let time = ui.input(|i| i.time);
        rig_ui::pump(self, ui.ctx(), time);
        rig_ui::window(self, ui.ctx(), time);
        live_ui::pump(self, ui.ctx(), time);
        audio_ui::pump(self, ui.ctx());
        live_ui::window(self, ui.ctx(), time);
        help::window(self, ui.ctx());
        stage_display::show(self, ui.ctx());
        self.paste_window(ui.ctx());
        if self.live.following() {
            live_ui::follow_view(self, ui);
            return;
        }
        if self.perform.is_some() {
            perform::show(self, ui);
            return;
        }
        let now = ui.input(|i| i.time);
        self.watch_disk(now);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(3));
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        let (undo, redo) = ui.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z),
            )
        });
        if !typing && undo {
            self.run("edit.undo", Value::Null);
            self.song_draft = None;
        }
        if !typing && redo {
            self.run("edit.redo", Value::Null);
            self.song_draft = None;
        }
        egui::Panel::top("top").show(ui, |ui| self.top_bar(ui));
        if let Some((msg, is_error)) = self.message.clone() {
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let color = if is_error {
                        ui.visuals().error_fg_color
                    } else {
                        ui.visuals().text_color()
                    };
                    ui.colored_label(color, msg);
                    if ui.small_button(tr!("Close")).clicked() {
                        self.message = None;
                    }
                });
            });
        }
        if self.engine.is_some() {
            egui::Panel::left("library")
                .default_size(260.0)
                .resizable(true)
                .show(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| match (self.engine.is_some(), self.view) {
            (false, _) => self.welcome(ui),
            (true, View::Songs) => match self.song {
                Some(id) if self.song_draft.is_some() => song_view::editor(self, ui, id),
                Some(id) => song_view::view(self, ui, id),
                None => {
                    ui.label(tr!("Pick a song, or create one with “New song”."));
                }
            },
            (true, View::Setlists) => match self.setlist {
                Some(id) => setlist_view::view(self, ui, id),
                None => {
                    ui.label(tr!("Pick a setlist, or create one with “New setlist”."));
                }
            },
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(e) = &self.engine {
            storage.set_string(LAST_LIBRARY_KEY, e.library().root().display().to_string());
        }
        storage.set_string(FONT_SIZE_KEY, self.font_size.to_string());
        storage.set_string(LANGUAGE_KEY, self.language.store());
        live_ui::save(self, storage);
        audio_ui::save_settings(self, storage);
        if let Ok(km) = serde_json::to_string(&self.keymap) {
            storage.set_string(KEYMAP_KEY, km);
        }
        if let Ok(rig) = serde_json::to_string(self.rig.config()) {
            storage.set_string(RIG_KEY, rig);
            self.rig_dirty = false;
        }
    }
}
