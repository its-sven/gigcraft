//! Perform mode: the stage view of a running setlist.
//!
//! Big type, current and next song, the parts with the current one marked,
//! the gig note, and the chart, transposed to the key set for this gig.
//! Keys and page turners act through the keymap; a press on a key that also
//! has a hold binding waits until it is released or held long enough.

use crate::{GigCraftApp, chart_view};
use gigcraft_model::{SetlistId, SongId};
use gigcraft_perform::{Action, Gesture};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Seconds a key must be held to count as "hold".
const HOLD_SECS: f64 = 0.6;

#[derive(Debug, Default, Clone)]
pub(crate) struct PerformUi {
    state: Value,
    /// Key name → (time pressed, hold already fired).
    pressed: HashMap<String, (f64, bool)>,
    scroll: f32,
    pending_scroll: Option<f32>,
    view_height: f32,
    break_until: Option<f64>,
    started_at: Option<f64>,
    last_sent: Vec<String>,
}

impl PerformUi {
    /// The last `perform.*` result (position, current and next item).
    pub(crate) fn state(&self) -> &Value {
        &self.state
    }
}

pub(crate) fn start(app: &mut GigCraftApp, setlist: SetlistId, index: Option<usize>) {
    let mut params = json!({ "setlist": setlist.to_string() });
    if let Some(i) = index {
        params["index"] = json!(i);
    }
    if let Some(out) = app.run("perform.start", params) {
        let mut ui = PerformUi::default();
        apply_effects(&mut ui, &out.result, 0.0);
        rig_effects(app, &out.result, 0.0);
        ui.state = out.result;
        app.perform = Some(ui);
    }
}

pub(crate) fn act(app: &mut GigCraftApp, action: Action, now: f64) {
    let name = serde_json::to_value(action).unwrap_or(Value::Null);
    let Some(out) = app.run("perform.action", json!({ "action": name })) else {
        return;
    };
    if out.result["running"] == false {
        app.perform = None;
        return;
    }
    rig_effects(app, &out.result, now);
    if let Some(p) = app.perform.as_mut() {
        apply_effects(p, &out.result, now);
        p.state = out.result;
    }
}

fn goto(app: &mut GigCraftApp, index: usize, part: usize, now: f64) {
    if let Some(out) = app.run("perform.goto", json!({ "index": index, "part": part })) {
        rig_effects(app, &out.result, now);
        if let Some(p) = app.perform.as_mut() {
            apply_effects(p, &out.result, now);
            p.state = out.result;
        }
    }
}

/// Effects for the rig (MIDI) after a perform command.
fn rig_effects(app: &mut GigCraftApp, result: &Value, now: f64) {
    let ms = crate::rig_ui::now_ms(now);
    for eff in result["effects"].as_array().into_iter().flatten() {
        match eff["type"].as_str().unwrap_or_default() {
            "send_part" => {
                let actions: Vec<gigcraft_model::MidiAction> =
                    serde_json::from_value(eff["actions"].clone()).unwrap_or_default();
                let outs = app.rig.send_part(&actions, ms);
                crate::rig_ui::handle(app, outs, now);
            }
            "panic" => {
                let outs = app.rig.panic(ms);
                crate::rig_ui::handle(app, outs, now);
            }
            _ => {}
        }
    }
}

fn apply_effects(p: &mut PerformUi, result: &Value, now: f64) {
    for eff in result["effects"].as_array().into_iter().flatten() {
        match eff["type"].as_str().unwrap_or_default() {
            "show" => {
                p.pending_scroll = Some(0.0);
                p.break_until = None;
            }
            "break" => p.break_until = Some(now + eff["minutes"].as_f64().unwrap_or(0.0) * 60.0),
            "send_part" => {
                // MIDI output is wired in by the platform layer; show what would be sent.
                p.last_sent = eff["actions"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|a| {
                        let msg: gigcraft_model::MidiAction =
                            serde_json::from_value(a.clone()).ok()?;
                        Some(format!("{}: {}", msg.device, msg.message))
                    })
                    .collect();
            }
            "page_forward" => p.pending_scroll = Some(p.scroll + p.view_height * 0.85),
            "page_back" => p.pending_scroll = Some((p.scroll - p.view_height * 0.85).max(0.0)),
            _ => {}
        }
    }
}

/// Keys from this frame → actions (press, double press is not used yet, hold).
fn key_actions(app: &mut GigCraftApp, ui: &egui::Ui, now: f64) -> Vec<Action> {
    let mut out = Vec::new();
    let events = ui.input(|i| i.events.clone());
    let Some(p) = app.perform.as_mut() else {
        return out;
    };
    for ev in events {
        if let egui::Event::Key {
            key,
            pressed,
            repeat,
            ..
        } = ev
        {
            if repeat {
                continue;
            }
            let name = key.name();
            if !app.keymap.has_extended_gesture(name) {
                if pressed && let Some(a) = app.keymap.action(name, Gesture::Press) {
                    out.push(a);
                }
                continue;
            }
            if pressed {
                p.pressed.insert(name.to_string(), (now, false));
            } else if let Some((t0, fired)) = p.pressed.remove(name)
                && !fired
            {
                let gesture = if now - t0 >= HOLD_SECS {
                    Gesture::Hold
                } else {
                    Gesture::Press
                };
                if let Some(a) = app.keymap.action(name, gesture) {
                    out.push(a);
                }
            }
        }
    }
    // Fire "hold" while the key is still down, so the musician gets feedback.
    for (name, (t0, fired)) in &mut p.pressed {
        if !*fired && now - *t0 >= HOLD_SECS {
            *fired = true;
            if let Some(a) = app.keymap.action(name, Gesture::Hold) {
                out.push(a);
            }
        }
    }
    if !p.pressed.is_empty() {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(50));
    }
    out
}

pub(crate) fn show(app: &mut GigCraftApp, ui: &mut egui::Ui) {
    let now = ui.input(|i| i.time);
    if let Some(p) = app.perform.as_mut()
        && p.started_at.is_none()
    {
        p.started_at = Some(now);
    }
    for action in key_actions(app, ui, now) {
        act(app, action, now);
    }
    let Some(p) = app.perform.clone() else {
        return;
    };
    let p = &p;
    let st = p.state.clone();
    let size = app.font_size * 1.25;
    let mut clicked: Option<Action> = None;
    let mut jump: Option<(usize, usize)> = None;

    egui::Panel::top("perform-top").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(st["name"].as_str().unwrap_or_default()).strong());
            ui.label(format!(
                "{} / {}",
                st["index"].as_u64().unwrap_or(0) + 1,
                st["count"].as_u64().unwrap_or(0)
            ));
            if let Some(t0) = p.started_at {
                let secs = (now - t0).max(0.0) as u32;
                ui.label(tr!(
                    "on stage {time}",
                    time = gigcraft_model::format_duration(secs)
                ));
            }
            ui.separator();
            let current_song = st["current"]["song"].as_str().and_then(|s| s.parse().ok());
            crate::audio_ui::transport(app, ui, current_song);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr!("Exit"))
                    .on_hover_text(tr!("Escape key"))
                    .clicked()
                {
                    clicked = Some(Action::Exit);
                }
                if ui
                    .button(egui::RichText::new(tr!("PANIC")).color(ui.visuals().error_fg_color))
                    .on_hover_text(tr!("All notes off, stop"))
                    .clicked()
                {
                    clicked = Some(Action::Panic);
                }
            });
        });
    });
    let live_summary = app.live.summary();
    egui::Panel::bottom("perform-bottom").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let big = |t: &str| {
                egui::Button::new(egui::RichText::new(t).size(size))
                    .min_size(egui::vec2(160.0, size * 2.0))
            };
            if ui
                .add(big(tr!("Previous")))
                .on_hover_text(tr!("Previous part or song (Left arrow)"))
                .clicked()
            {
                clicked = Some(Action::Previous);
            }
            if ui
                .add(big(tr!("Next")))
                .on_hover_text(tr!(
                    "Next part or song (Right arrow or Space; hold for the next song)"
                ))
                .clicked()
            {
                clicked = Some(Action::Next);
            }
            ui.separator();
            let next = &st["next"];
            let next_text = match next["type"].as_str() {
                Some("song") => tr!(
                    "Up next: {title}",
                    title = format!(
                        "{}{}",
                        next["title"].as_str().unwrap_or_default(),
                        next["key"]
                            .as_str()
                            .map(|k| format!("  [{k}]"))
                            .unwrap_or_default()
                    )
                ),
                Some("break") => tr!("Up next: break {minutes} min", minutes = next["minutes"]),
                Some(_) => tr!("Up next: (missing song)").into(),
                None => tr!("Last song").into(),
            };
            ui.label(egui::RichText::new(next_text).size(size * 0.8));
            let devices: Vec<String> = app
                .rig
                .status()
                .into_iter()
                .map(|d| {
                    let what = d
                        .current
                        .map(|c| tr!("preset {number}", number = c))
                        .unwrap_or_else(|| crate::i18n::state(&d.state));
                    format!("{}: {what}{}", d.name, if d.busy { " …" } else { "" })
                })
                .collect();
            if let Some(live) = &live_summary {
                ui.separator();
                ui.label(egui::RichText::new(live).size(size * 0.6));
            }
            if !devices.is_empty() {
                ui.separator();
                ui.label(egui::RichText::new(devices.join("   ")).size(size * 0.6));
            } else if !p.last_sent.is_empty() {
                ui.separator();
                ui.label(
                    egui::RichText::new(tr!("Presets: {list}", list = p.last_sent.join("; ")))
                        .size(size * 0.55)
                        .weak(),
                );
            }
        });
        ui.add_space(6.0);
    });

    let mut new_scroll = None;
    let mut view_height = 0.0;
    egui::CentralPanel::default().show(ui, |ui| {
        crate::live_ui::cue_banner(&mut app.live, ui, now, size);
        let cur = &st["current"];
        match cur["type"].as_str() {
            Some("song") => {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(cur["title"].as_str().unwrap_or_default())
                            .size(size * 1.6)
                            .strong(),
                    );
                    if let Some(k) = cur["key"].as_str() {
                        ui.label(
                            egui::RichText::new(tr!("Key {key}", key = k))
                                .size(size)
                                .color(ui.visuals().hyperlink_color),
                        );
                    }
                    if let Some(t) = cur["tempo_bpm"].as_f64() {
                        ui.label(egui::RichText::new(format!("{t:.0} BPM")).size(size * 0.8));
                    }
                });
                let parts: Vec<String> = cur["part_names"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                if parts.len() > 1 {
                    let current_part = st["part"].as_u64().unwrap_or(0) as usize;
                    let index = st["index"].as_u64().unwrap_or(0) as usize;
                    ui.horizontal_wrapped(|ui| {
                        for (i, name) in parts.iter().enumerate() {
                            if ui
                                .selectable_label(
                                    i == current_part,
                                    egui::RichText::new(name).size(size * 0.8),
                                )
                                .on_hover_text(tr!("Jump to this part and send its presets"))
                                .clicked()
                            {
                                jump = Some((index, i));
                            }
                        }
                    });
                }
                if let Some(n) = cur["gig_notes"].as_str().filter(|n| !n.is_empty()) {
                    ui.label(
                        egui::RichText::new(n)
                            .size(size)
                            .color(ui.visuals().warn_fg_color),
                    );
                }
                ui.separator();
                let song: Option<SongId> = cur["song"].as_str().and_then(|s| s.parse().ok());
                if let (Some(id), Some(engine)) = (song, &app.engine) {
                    let lib = engine.library();
                    let gig_key = cur["key"].as_str();
                    let song_key = lib.song(id).and_then(|s| s.key.clone());
                    let semis = match (song_key.as_deref(), gig_key) {
                        (Some(from), Some(to)) => {
                            gigcraft_chordpro::semitones_between(from, to).unwrap_or(0)
                        }
                        _ => 0,
                    };
                    let chart = app.chart_cache.get(lib, id, semis);
                    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
                    if let Some(o) = p.pending_scroll {
                        area = area.vertical_scroll_offset(o);
                    }
                    let out = area.show(ui, |ui| chart_view::show(ui, &chart.chart, size));
                    new_scroll = Some(out.state.clamped_offset().y);
                    view_height = out.inner_rect.height();
                }
            }
            Some("break") => {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    let label = cur["label"]
                        .as_str()
                        .filter(|l| !l.is_empty())
                        .unwrap_or(tr!("Break"));
                    ui.label(egui::RichText::new(label).size(size * 2.0).strong());
                    if let Some(until) = p.break_until {
                        let left = (until - now).max(0.0) as u32;
                        ui.label(
                            egui::RichText::new(gigcraft_model::format_duration(left))
                                .size(size * 3.0),
                        );
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_millis(250));
                    }
                });
            }
            Some(_) => {
                ui.label(
                    egui::RichText::new(tr!("This song is no longer in the library.")).size(size),
                );
            }
            None => {
                ui.label(egui::RichText::new(tr!("This setlist is empty.")).size(size));
            }
        }
    });

    if let Some(p) = app.perform.as_mut() {
        p.pending_scroll = None;
        if let Some(s) = new_scroll {
            p.scroll = s;
        }
        if view_height > 0.0 {
            p.view_height = view_height;
        }
    }
    if let Some((i, part)) = jump {
        goto(app, i, part, now);
    }
    if let Some(a) = clicked {
        act(app, a, now);
    }
    // Keep the "on stage" clock running.
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_secs(1));
}
