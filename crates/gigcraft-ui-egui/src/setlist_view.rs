//! Setlist view and editor.

use crate::{GigCraftApp, View, perform};
use gigcraft_model::SetlistId;
use serde_json::{Value, json};
use std::collections::HashMap;

/// Editor state of the selected setlist.
#[derive(Debug, Default)]
pub(crate) struct State {
    pub(crate) editing: bool,
    add_query: String,
    /// Text buffers of fields being typed in, keyed by (set, entry, field).
    buffers: HashMap<(usize, usize, &'static str), String>,
    name: Option<String>,
    date: Option<String>,
}

impl State {
    /// State with the editor switched on (for a new setlist).
    pub(crate) fn editing() -> Self {
        Self {
            editing: true,
            ..Default::default()
        }
    }
}

pub(crate) fn view(app: &mut GigCraftApp, ui: &mut egui::Ui, id: SetlistId) {
    let Some(out) = app.run("setlist.get", json!({ "id": id.to_string() })) else {
        return;
    };
    let d = out.result;
    let size = app.font_size;
    let editing = app.setlist_ui.editing;
    let mut cmds: Vec<(&'static str, Value)> = Vec::new();
    let sid = id.to_string();

    ui.horizontal(|ui| {
        if editing {
            let name = app
                .setlist_ui
                .name
                .get_or_insert_with(|| d["name"].as_str().unwrap_or_default().to_string());
            let r = ui.add(
                egui::TextEdit::singleline(name)
                    .font(egui::TextStyle::Heading)
                    .desired_width(320.0),
            );
            if r.lost_focus() && name.trim() != d["name"].as_str().unwrap_or_default() {
                cmds.push((
                    "setlist.update",
                    json!({"id": sid, "fields": {"name": name.trim()}}),
                ));
            }
            ui.label(tr!("Date"));
            let date = app
                .setlist_ui
                .date
                .get_or_insert_with(|| d["date"].as_str().unwrap_or_default().to_string());
            let r = ui.add(
                egui::TextEdit::singleline(date)
                    .hint_text(tr!("YYYY-MM-DD"))
                    .desired_width(100.0),
            );
            if r.lost_focus() && date.trim() != d["date"].as_str().unwrap_or_default() {
                let v = if date.trim().is_empty() {
                    Value::Null
                } else {
                    json!(date.trim())
                };
                cmds.push(("setlist.update", json!({"id": sid, "fields": {"date": v}})));
            }
        } else {
            ui.heading(
                egui::RichText::new(d["name"].as_str().unwrap_or_default()).size(size * 1.4),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(egui::RichText::new(tr!("Perform")).strong())
                .on_hover_text(tr!("Stage view for this setlist"))
                .clicked()
            {
                cmds.push(("__perform", Value::Null));
            }
            let label = if editing { tr!("Done") } else { tr!("Edit") };
            if ui
                .button(label)
                .on_hover_text(tr!(
                    "Add, move and remove songs and breaks; set the key and a note for this gig"
                ))
                .clicked()
            {
                cmds.push(("__toggle_edit", Value::Null));
            }
            if ui
                .button(tr!("Copy as text"))
                .on_hover_text(tr!("For mail, chat or printing"))
                .clicked()
            {
                cmds.push(("__copy", Value::Null));
            }
            if editing
                && ui
                    .button(tr!("Move to trash"))
                    .on_hover_text(tr!(
                        "Move this setlist to the library's trash; Undo brings it back"
                    ))
                    .clicked()
            {
                cmds.push(("setlist.trash", json!({"id": sid})));
            }
        });
    });
    let mut summary = tr!(
        "{date}  ·  {total} total",
        date = d["date"].as_str().unwrap_or(tr!("no date")),
        total = d["total"].as_str().unwrap_or_default()
    );
    if let Some(n) = d["unknown_durations"].as_u64().filter(|n| *n > 0) {
        summary.push(' ');
        summary.push_str(&trn!(
            n,
            "(+{n} song without length)",
            "(+{n} songs without length)"
        ));
    }
    ui.label(summary);
    if let Some(n) = d["notes"].as_str().filter(|n| !n.is_empty()) {
        ui.label(egui::RichText::new(n).italics());
    }
    ui.separator();

    let sets = d["sets"].as_array().cloned().unwrap_or_default();
    let set_count = sets.len();
    let mut open_song = None;
    let mut perform_at = None;
    let mut item_index = 0usize;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (si, set) in sets.iter().enumerate() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "{}  ({})",
                        set["name"].as_str().unwrap_or_default(),
                        set["duration"].as_str().unwrap_or_default()
                    ))
                    .size(size * 1.1)
                    .strong(),
                );
                if editing {
                    if ui.small_button(tr!("Add break")).on_hover_text(tr!("Add a break with a countdown at the end of this set")).clicked() {
                        cmds.push(("setlist.add_break", json!({"id": sid, "set": si, "minutes": 15})));
                    }
                    if set_count > 1 && ui.small_button(tr!("Remove set")).on_hover_text(tr!("Remove this set and its entries")).clicked() {
                        cmds.push(("setlist.remove_set", json!({"id": sid, "set": si})));
                    }
                }
            });
            let entries = set["entries"].as_array().cloned().unwrap_or_default();
            let len = entries.len();
            let mut n = 0;
            for (ei, e) in entries.iter().enumerate() {
                let this_item = item_index;
                item_index += 1;
                ui.horizontal(|ui| {
                    if editing {
                        if ui.add_enabled(ei > 0, egui::Button::new(tr!("Up")).small()).on_hover_text(tr!("Move up")).clicked() {
                            cmds.push(("setlist.move_entry", json!({"id": sid, "from_set": si, "from": ei, "to": ei - 1})));
                        }
                        if ui.add_enabled(ei + 1 < len, egui::Button::new(tr!("Down")).small()).on_hover_text(tr!("Move down")).clicked() {
                            cmds.push(("setlist.move_entry", json!({"id": sid, "from_set": si, "from": ei, "to": ei + 1})));
                        }
                        if si + 1 < set_count && ui.small_button(tr!("To next set")).on_hover_text(tr!("Move to the start of the next set")).clicked() {
                            cmds.push(("setlist.move_entry", json!({"id": sid, "from_set": si, "from": ei, "to_set": si + 1, "to": 0})));
                        }
                        if ui.small_button(tr!("Remove")).on_hover_text(tr!("Remove from this setlist – the song stays in the library")).clicked() {
                            cmds.push(("setlist.remove_entry", json!({"id": sid, "set": si, "index": ei})));
                        }
                    }
                    if e["type"] == "break" {
                        let label = e["label"].as_str().filter(|l| !l.is_empty()).unwrap_or(tr!("Break"));
                        if editing {
                            let mut minutes = e["minutes"].as_u64().unwrap_or(0) as u32;
                            ui.label(format!("— {label}"));
                            if ui.add(egui::DragValue::new(&mut minutes).range(0..=240).suffix(" min")).changed() {
                                cmds.push(("setlist.update_entry", json!({"id": sid, "set": si, "index": ei, "fields": {"minutes": minutes}})));
                            }
                        } else {
                            ui.label(egui::RichText::new(format!("— {label} {} min —", e["minutes"])).size(size * 0.8).weak());
                        }
                        return;
                    }
                    n += 1;
                    let missing = e["missing"] == true;
                    let title = e["title"].as_str().unwrap_or(tr!("(missing song)"));
                    let mut text = egui::RichText::new(format!("{n}. {title}")).size(size);
                    if missing {
                        text = text.strikethrough().weak();
                    }
                    let r = ui
                        .add_enabled(!missing, egui::Button::new(text).frame(false))
                        .on_hover_text(tr!("Open the song; right-click to perform from here"));
                    if r.clicked()
                        && let Some(id) = e["song"].as_str().and_then(|s| s.parse().ok())
                    {
                        open_song = Some(id);
                    }
                    r.context_menu(|ui| {
                        if ui.button(tr!("Perform from here")).clicked() {
                            perform_at = Some(this_item);
                            ui.close();
                        }
                    });
                    if editing {
                        let ui_state = &mut app.setlist_ui;
                        let key = ui_state
                            .buffers
                            .entry((si, ei, "key"))
                            .or_insert_with(|| e["key"].as_str().unwrap_or_default().to_string());
                        ui.label(tr!("Key"));
                        let r = ui.add(egui::TextEdit::singleline(key).desired_width(40.0));
                        if r.lost_focus() && key.trim() != e["key"].as_str().unwrap_or_default() {
                            cmds.push(("setlist.update_entry", json!({"id": sid, "set": si, "index": ei, "fields": {"key": key.trim()}})));
                        }
                        let notes = ui_state
                            .buffers
                            .entry((si, ei, "notes"))
                            .or_insert_with(|| e["notes"].as_str().unwrap_or_default().to_string());
                        let r = ui.add(egui::TextEdit::singleline(notes).hint_text(tr!("Note for this gig")).desired_width((ui.available_width() - 110.0).clamp(80.0, 240.0)));
                        if r.lost_focus() && notes.as_str() != e["notes"].as_str().unwrap_or_default() {
                            cmds.push(("setlist.update_entry", json!({"id": sid, "set": si, "index": ei, "fields": {"notes": notes.clone()}})));
                        }
                        let mut optional = e[tr!("optional")] == true;
                        if ui.checkbox(&mut optional, tr!("optional")).on_hover_text(tr!("Play only if there is time")).changed() {
                            cmds.push(("setlist.update_entry", json!({"id": sid, "set": si, "index": ei, "fields": {"optional": optional}})));
                        }
                    } else {
                        if let Some(k) = e["key"].as_str() {
                            ui.label(egui::RichText::new(format!("[{k}]")).size(size));
                        }
                        if e[tr!("optional")] == true {
                            ui.label(egui::RichText::new(tr!("(optional)")).size(size * 0.8).weak());
                        }
                    }
                });
                if !editing
                    && let Some(note) = e["notes"].as_str().filter(|x| !x.is_empty())
                {
                    ui.label(egui::RichText::new(format!("    {note}")).size(size * 0.75).italics());
                }
            }
        }
        if editing {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(tr!("Add set")).on_hover_text(tr!("Add another set, e.g. an encore")).clicked() {
                    cmds.push(("setlist.add_set", json!({"id": sid})));
                }
            });
            ui.add_space(8.0);
            ui.strong(tr!("Add songs"));
            ui.add(egui::TextEdit::singleline(&mut app.setlist_ui.add_query).hint_text(tr!("Search the library")));
            if let Some(engine) = &app.engine {
                for song in engine.library().search(&app.setlist_ui.add_query).into_iter().take(12) {
                    ui.horizontal(|ui| {
                        if ui.small_button(tr!("Add")).on_hover_text(tr!("Add to the end of this setlist")).clicked() {
                            cmds.push(("setlist.add_song", json!({"id": sid, "song": song.id.to_string()})));
                        }
                        ui.label(song.display_name());
                    });
                }
            }
        }
    });

    for (cmd, params) in cmds {
        match cmd {
            "__toggle_edit" => {
                app.setlist_ui = State {
                    editing: !editing,
                    ..Default::default()
                };
            }
            "__perform" => perform::start(app, id, None),
            "__copy" => {
                let text = app
                    .run("export.setlist_text", json!({"id": sid}))
                    .and_then(|o| o.result["text"].as_str().map(str::to_string))
                    .unwrap_or_else(|| setlist_text(&d));
                ui.ctx().copy_text(text);
                app.notify(tr!("Setlist copied as text."));
            }
            "setlist.trash" => {
                if app.run(cmd, params).is_some() {
                    app.setlist = None;
                    app.notify(tr!(
                        "Setlist moved to the library's trash (Undo brings it back)."
                    ));
                }
            }
            _ => {
                app.run(cmd, params);
                app.setlist_ui.buffers.clear();
                app.setlist_ui.name = None;
                app.setlist_ui.date = None;
            }
        }
    }
    if let Some(idx) = perform_at {
        perform::start(app, id, Some(idx));
    }
    if let Some(id) = open_song {
        app.song = Some(id);
        app.view = View::Songs;
        app.transpose = 0;
    }
}

/// The setlist as plain text, for sharing and printing.
fn setlist_text(d: &Value) -> String {
    let mut out = d["name"].as_str().unwrap_or_default().to_string();
    if let Some(date) = d["date"].as_str() {
        out.push_str(&format!(" – {date}"));
    }
    out.push('\n');
    for set in d["sets"].as_array().into_iter().flatten() {
        out.push_str(&format!(
            "\n{} ({})\n",
            set["name"].as_str().unwrap_or_default(),
            set["duration"].as_str().unwrap_or_default()
        ));
        let mut n = 0;
        for e in set["entries"].as_array().into_iter().flatten() {
            if e["type"] == "break" {
                out.push_str(&format!(
                    "   — {} {} min —\n",
                    e["label"]
                        .as_str()
                        .filter(|l| !l.is_empty())
                        .unwrap_or("Break"),
                    e["minutes"]
                ));
                continue;
            }
            n += 1;
            out.push_str(&format!(
                "{n:>2}. {}",
                e["title"].as_str().unwrap_or("(missing)")
            ));
            if let Some(k) = e["key"].as_str() {
                out.push_str(&format!("  [{k}]"));
            }
            if e[tr!("optional")] == true {
                out.push_str("  (optional)");
            }
            if let Some(note) = e["notes"].as_str().filter(|x| !x.is_empty()) {
                out.push_str(&format!("  – {note}"));
            }
            out.push('\n');
        }
    }
    out.push_str(&format!(
        "\nTotal {}\n",
        d["total"].as_str().unwrap_or_default()
    ));
    out
}
