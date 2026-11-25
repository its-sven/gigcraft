//! GigCraft's desktop and web interface in egui.
//!
//! Library browser and editors for songs and setlists, and perform mode for
//! the stage. Every change goes through `gigcraft-commands`, like the CLI and
//! MCP. Platform services (folder dialogs) are passed in as [`Hooks`], so this
//! crate also builds for the web.

mod chart_view;
mod perform;
mod setlist_view;
mod song_view;

use gigcraft_commands::{Engine, Outcome};
use gigcraft_model::{SetlistId, SongId};
use gigcraft_perform::Keymap;
use gigcraft_store::Library;
use serde_json::{Value, json};
use std::path::PathBuf;

const LAST_LIBRARY_KEY: &str = "gigcraft.last_library";
const FONT_SIZE_KEY: &str = "gigcraft.font_size";
const KEYMAP_KEY: &str = "gigcraft.keymap";

/// Platform services the shell can offer.
#[derive(Default)]
pub struct Hooks {
    /// Ask the user for a folder (open or create a library).
    pub pick_folder: Option<Box<dyn Fn() -> Option<PathBuf>>>,
    /// Ask the user for a file (attachments, imports).
    pub pick_file: Option<Box<dyn Fn() -> Option<PathBuf>>>,
}

impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks")
            .field("pick_folder", &self.pick_folder.is_some())
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
}

impl GigCraftApp {
    /// Create the app; opens `start.library` or the last library used.
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start, hooks: Hooks) -> Self {
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
        };
        if let Some(storage) = cc.storage {
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
            let last = storage.get_string(LAST_LIBRARY_KEY).map(PathBuf::from);
            if start.library.is_none()
                && let Some(path) = last.filter(|p| p.is_dir())
            {
                app.open(path);
            }
        }
        if let Some(path) = start.library.clone() {
            app.open(path);
        }
        app.select(&start);
        app
    }

    fn select(&mut self, start: &Start) {
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
                        format!("{problems} file(s) could not be read – see library.info"),
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
                    format!("Demo library created in {}", target.display()),
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
                self.notify(format!(
                    "Updated from disk ({changed} change(s) from another device or program)."
                ));
            }
        }
    }

    pub(crate) fn notify(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), false));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("GigCraft");
            ui.separator();
            let mut open = None;
            let mut demo = None;
            if let Some(pick) = &self.hooks.pick_folder {
                if ui.button("Open library…").clicked() {
                    open = pick();
                }
                if ui
                    .button("New demo library…")
                    .on_hover_text("Creates a folder “GigCraft Demo” in the folder you pick")
                    .clicked()
                {
                    demo = pick();
                }
                ui.separator();
            }
            if let Some(p) = open {
                self.open(p);
            }
            if let Some(p) = demo {
                self.create_demo(p);
            }
            let (undo, redo) = self.engine.as_ref().map_or((None, None), |e| {
                (
                    e.undo_label().map(str::to_string),
                    e.redo_label().map(str::to_string),
                )
            });
            if ui
                .add_enabled(undo.is_some(), egui::Button::new("Undo"))
                .on_hover_text(undo.unwrap_or_default())
                .clicked()
            {
                self.run("edit.undo", Value::Null);
                self.song_draft = None;
            }
            if ui
                .add_enabled(redo.is_some(), egui::Button::new("Redo"))
                .on_hover_text(redo.unwrap_or_default())
                .clicked()
            {
                self.run("edit.redo", Value::Null);
                self.song_draft = None;
            }
            ui.separator();
            ui.label("Text");
            ui.add(egui::Slider::new(&mut self.font_size, 12.0..=48.0).show_value(false));
            if let Some(e) = &self.engine {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(&e.library().info().name).strong());
                });
            }
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.view, View::Songs, "Songs");
            ui.selectable_value(&mut self.view, View::Setlists, "Setlists");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, cmd) = match self.view {
                    View::Songs => ("New song", "song"),
                    View::Setlists => ("New setlist", "setlist"),
                };
                if ui.small_button(label).clicked() {
                    self.create(cmd);
                }
            });
        });
        ui.separator();
        let Some(engine) = &self.engine else { return };
        let mut pick_song = None;
        let mut pick_list = None;
        match self.view {
            View::Songs => {
                ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .hint_text("Search: words, \"phrase\", -not"),
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
                if let Some(out) = self.run("song.create", json!({"title": "New song", "chart": "{title: New song}\n\n[C]Lyrics with [G]chords\n"}))
                    && let Some(id) = out.result["id"].as_str().and_then(|s| s.parse().ok())
                {
                    self.song = Some(id);
                    self.view = View::Songs;
                    self.song_draft = song_view::Draft::load(self, id);
                }
            }
            _ => {
                if let Some(out) = self.run("setlist.create", json!({"name": "New setlist"}))
                    && let Some(id) = out.result["id"].as_str().and_then(|s| s.parse().ok())
                {
                    self.setlist = Some(id);
                    self.view = View::Setlists;
                    self.setlist_ui = setlist_view::State::editing();
                }
            }
        }
    }

    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(80.0);
            ui.heading("Welcome to GigCraft");
            ui.label("Open a library folder, or create the demo library to look around.");
            if self.hooks.pick_folder.is_none() {
                ui.label("Start with: gigcraft <library folder>");
            }
        });
    }
}

impl eframe::App for GigCraftApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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
                    if ui.small_button("Close").clicked() {
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
                    ui.label("Pick a song, or create one with “New song”.");
                }
            },
            (true, View::Setlists) => match self.setlist {
                Some(id) => setlist_view::view(self, ui, id),
                None => {
                    ui.label("Pick a setlist, or create one with “New setlist”.");
                }
            },
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(e) = &self.engine {
            storage.set_string(LAST_LIBRARY_KEY, e.library().root().display().to_string());
        }
        storage.set_string(FONT_SIZE_KEY, self.font_size.to_string());
        if let Ok(km) = serde_json::to_string(&self.keymap) {
            storage.set_string(KEYMAP_KEY, km);
        }
    }
}
