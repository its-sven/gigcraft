//! GigCraft's desktop and web interface in egui.
//!
//! Phase 0 shell: open a library, browse and search songs, read charts with
//! transposition, browse setlists with sets, breaks and running times, undo
//! and redo. Every change goes through `gigcraft-commands`, like the CLI and
//! MCP. Platform services (folder dialogs) are passed in as [`Hooks`], so this
//! crate also builds for the web.

mod chart_view;

use gigcraft_commands::{Engine, Outcome};
use gigcraft_model::{SetlistId, SongId, format_duration};
use gigcraft_store::Library;
use serde_json::{Value, json};
use std::path::PathBuf;

const LAST_LIBRARY_KEY: &str = "gigcraft.last_library";
const FONT_SIZE_KEY: &str = "gigcraft.font_size";

/// Platform services the shell can offer.
#[derive(Default)]
pub struct Hooks {
    /// Ask the user for a folder (open or create a library).
    pub pick_folder: Option<Box<dyn Fn() -> Option<PathBuf>>>,
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
}

impl GigCraftApp {
    /// Create the app; opens `library` or the last library used.
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start, hooks: Hooks) -> Self {
        let library = start.library.clone();
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
        };
        if let Some(storage) = cc.storage {
            if let Some(size) = storage
                .get_string(FONT_SIZE_KEY)
                .and_then(|s| s.parse().ok())
            {
                app.font_size = size;
            }
            let last = storage.get_string(LAST_LIBRARY_KEY).map(PathBuf::from);
            if library.is_none()
                && let Some(path) = last.filter(|p| p.is_dir())
            {
                app.open(path);
            }
        }
        if let Some(path) = library {
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
    }

    fn open(&mut self, path: PathBuf) {
        match Library::open(&path) {
            Ok(lib) => {
                let problems = lib.problems().len();
                self.engine = Some(Engine::new(lib));
                self.song = None;
                self.setlist = None;
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
    fn run(&mut self, id: &str, params: Value) -> Option<Outcome> {
        let engine = self.engine.as_mut()?;
        match engine.run(id, &params) {
            Ok(out) => Some(out),
            Err(e) => {
                self.message = Some((e.to_string(), true));
                None
            }
        }
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
            }
            if ui
                .add_enabled(redo.is_some(), egui::Button::new("Redo"))
                .on_hover_text(redo.unwrap_or_default())
                .clicked()
            {
                self.run("edit.redo", Value::Null);
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
        });
        ui.separator();
        let Some(engine) = &self.engine else { return };
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
                                self.song = Some(song.id);
                                self.transpose = 0;
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
                                self.setlist = Some(list.id);
                            }
                        }
                    });
            }
        }
    }

    fn song_view(&mut self, ui: &mut egui::Ui, id: SongId) {
        let Some(engine) = &self.engine else { return };
        let lib = engine.library();
        let Some(song) = lib.song(id) else {
            ui.label("This song is no longer in the library.");
            return;
        };
        ui.horizontal(|ui| {
            ui.heading(egui::RichText::new(&song.title).size(self.font_size * 1.4));
            if !song.artist.is_empty() {
                ui.label(egui::RichText::new(&song.artist).size(self.font_size));
            }
        });
        let chart = self.chart_cache.get(lib, id, self.transpose);
        ui.horizontal(|ui| {
            let mut facts = Vec::new();
            if let Some(k) = chart.chart.key.clone().or_else(|| song.key.clone()) {
                facts.push(format!("Key {k}"));
            }
            if let Some(t) = song.tempo_bpm {
                facts.push(format!("{t:.0} BPM"));
            }
            if let Some(ts) = &song.time_signature {
                facts.push(ts.clone());
            }
            if let Some(c) = song.capo.filter(|c| *c > 0) {
                facts.push(format!("Capo {c}"));
            }
            if let Some(d) = song.duration_secs {
                facts.push(format_duration(d));
            }
            ui.label(facts.join("  ·  "));
            ui.separator();
            if ui.button("−").on_hover_text("Transpose down").clicked() {
                self.transpose = if self.transpose <= -11 {
                    0
                } else {
                    self.transpose - 1
                };
            }
            ui.label(format!("{:+}", self.transpose));
            if ui.button("+").on_hover_text("Transpose up").clicked() {
                self.transpose = if self.transpose >= 11 {
                    0
                } else {
                    self.transpose + 1
                };
            }
        });
        for part in &song.parts {
            let actions: Vec<String> = part
                .actions
                .iter()
                .map(|a| format!("{}: {}", a.device, a.message))
                .collect();
            if !actions.is_empty() {
                ui.small(format!("{}  –  {}", part.name, actions.join("; ")));
            }
        }
        if !song.notes.is_empty() {
            ui.label(egui::RichText::new(&song.notes).italics());
        }
        ui.separator();
        for w in &chart.warnings {
            ui.colored_label(ui.visuals().warn_fg_color, w);
        }
        let font = self.font_size;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                chart_view::show(ui, &chart.chart, font);
            });
    }

    fn setlist_view(&mut self, ui: &mut egui::Ui, id: SetlistId) {
        let Some(out) = self.run("setlist.get", json!({ "id": id.to_string() })) else {
            return;
        };
        let d = out.result;
        ui.heading(
            egui::RichText::new(d["name"].as_str().unwrap_or_default()).size(self.font_size * 1.4),
        );
        ui.label(format!(
            "{}  ·  {} total{}",
            d["date"].as_str().unwrap_or("no date"),
            d["total"].as_str().unwrap_or_default(),
            match d["unknown_durations"].as_u64() {
                Some(n) if n > 0 => format!(" (+{n} songs without length)"),
                _ => String::new(),
            }
        ));
        if let Some(n) = d["notes"].as_str().filter(|n| !n.is_empty()) {
            ui.label(egui::RichText::new(n).italics());
        }
        ui.separator();
        let size = self.font_size;
        let mut open_song = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for set in d["sets"].as_array().into_iter().flatten() {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{}  ({})",
                            set["name"].as_str().unwrap_or_default(),
                            set["duration"].as_str().unwrap_or_default()
                        ))
                        .size(size * 1.1)
                        .strong(),
                    );
                    let mut n = 0;
                    for e in set["entries"].as_array().into_iter().flatten() {
                        if e["type"] == "break" {
                            ui.label(
                                egui::RichText::new(format!(
                                    "— {} {} min —",
                                    e["label"]
                                        .as_str()
                                        .filter(|l| !l.is_empty())
                                        .unwrap_or("Break"),
                                    e["minutes"]
                                ))
                                .size(size * 0.8)
                                .weak(),
                            );
                            continue;
                        }
                        n += 1;
                        let missing = e["missing"] == true;
                        let title = e["title"].as_str().unwrap_or("(missing song)");
                        let mut line = format!("{n}. {title}");
                        if let Some(k) = e["key"].as_str() {
                            line.push_str(&format!("   [{k}]"));
                        }
                        if e["optional"] == true {
                            line.push_str("   (optional)");
                        }
                        let mut text = egui::RichText::new(line).size(size);
                        if missing {
                            text = text.strikethrough().weak();
                        }
                        let r = ui.add_enabled(!missing, egui::Button::new(text).frame(false));
                        if r.clicked()
                            && let Some(id) = e["song"].as_str().and_then(|s| s.parse().ok())
                        {
                            open_song = Some(id);
                        }
                        if let Some(note) = e["notes"].as_str().filter(|x| !x.is_empty()) {
                            ui.label(
                                egui::RichText::new(format!("    {note}"))
                                    .size(size * 0.75)
                                    .italics(),
                            );
                        }
                    }
                }
            });
        if let Some(id) = open_song {
            self.song = Some(id);
            self.view = View::Songs;
            self.transpose = 0;
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
        let (undo, redo) = ui.input(|i| {
            let cmd = i.modifiers.command;
            (
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z),
            )
        });
        if undo {
            self.run("edit.undo", Value::Null);
        }
        if redo {
            self.run("edit.redo", Value::Null);
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
                Some(id) => self.song_view(ui, id),
                None => {
                    ui.label("Pick a song.");
                }
            },
            (true, View::Setlists) => match self.setlist {
                Some(id) => self.setlist_view(ui, id),
                None => {
                    ui.label("Pick a setlist.");
                }
            },
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(e) = &self.engine {
            storage.set_string(LAST_LIBRARY_KEY, e.library().root().display().to_string());
        }
        storage.set_string(FONT_SIZE_KEY, self.font_size.to_string());
    }
}
