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
    /// Where each chart section starts (from the last frame), for following the track.
    block_tops: Vec<f32>,
    /// The section the view last scrolled to for the backing track.
    auto_block: Option<usize>,
    /// Auto-scroll is on: the time of the last frame it moved the chart.
    auto_scroll: Option<f64>,
    /// Height of the whole chart (from the last frame).
    content_height: f32,
}

/// The chart block the backing track is in: the last section marker at or
/// before `pos`, matched to a block by its label ("Chorus", "Verse: 2").
pub(crate) fn section_at(
    song: &gigcraft_model::Song,
    chart: &gigcraft_chordpro::Chart,
    pos: f64,
) -> Option<usize> {
    let norm = |s: &str| s.trim().to_lowercase();
    let marker = song
        .markers
        .iter()
        .filter(|m| m.at_secs <= pos)
        .max_by(|a, b| a.at_secs.total_cmp(&b.at_secs))?;
    let want = norm(&marker.name);
    chart
        .blocks
        .iter()
        .position(|b| b.label.as_deref().is_some_and(|l| norm(l) == want))
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

/// The backing track reached `secs`: switch to the part of its section and
/// send that part's presets (the chart stays where it is).
pub(crate) fn track(app: &mut GigCraftApp, secs: f64, now: f64) {
    let Some(out) = app.run("perform.track_position", json!({ "secs": secs })) else {
        return;
    };
    if out.result["effects"].as_array().is_none_or(Vec::is_empty) {
        return;
    }
    rig_effects(app, &out.result, now);
    if let Some(p) = app.perform.as_mut() {
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
                crate::audio_ui::panic(app);
            }
            "playback" => crate::audio_ui::toggle(app),
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
                p.auto_scroll = None;
            }
            "auto_scroll" => {
                p.auto_scroll = if p.auto_scroll.is_some() {
                    None
                } else {
                    Some(now)
                };
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
    let factor = app.scroll_factor;
    let mut clicked: Option<Action> = None;
    let mut jump: Option<(usize, usize)> = None;
    let mut speed: Option<bool> = None;
    let mut content_height = 0.0;
    let mut auto_moved = None;

    egui::Panel::top("perform-top").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(st["name"].as_str().unwrap_or_default())
                    .strong()
                    .size(16.0),
            );
            crate::theme::readout(
                ui,
                tr!("Song"),
                &format!(
                    "{}/{}",
                    st["index"].as_u64().unwrap_or(0) + 1,
                    st["count"].as_u64().unwrap_or(0)
                ),
                15.0,
            );
            if let Some(t0) = p.started_at {
                let secs = (now - t0).max(0.0) as u32;
                crate::theme::readout(
                    ui,
                    tr!("On stage"),
                    &gigcraft_model::format_duration(secs),
                    15.0,
                );
            }
            ui.separator();
            let current_song = st["current"]["song"].as_str().and_then(|s| s.parse().ok());
            crate::audio_ui::transport(app, ui, current_song);
            ui.separator();
            // Auto-scroll: on/off and speed.
            let running = p.auto_scroll.is_some();
            let label = if running {
                tr!("Stop scrolling")
            } else {
                tr!("Auto-scroll")
            };
            let button = if running {
                crate::theme::primary(label)
            } else {
                egui::Button::new(label)
            };
            if crate::named(ui.add(button), tr!("Auto-scroll"))
                .on_hover_text(tr!(
                    "The chart scrolls by itself, as fast as the song is long (key S). Scroll by hand at any time."
                ))
                .clicked()
            {
                clicked = Some(Action::AutoScroll);
            }
            if crate::named(ui.small_button("−"), tr!("Scroll slower"))
                .on_hover_text(tr!("Scroll slower"))
                .clicked()
            {
                speed = Some(false);
            }
            crate::theme::readout(ui, tr!("Speed"), &format!("{:.0}%", app.scroll_factor * 100.0), 13.0);
            if crate::named(ui.small_button("+"), tr!("Scroll faster"))
                .on_hover_text(tr!("Scroll faster"))
                .clicked()
            {
                speed = Some(true);
            }
            ui.separator();
            crate::live_ui::mark_buttons(app, ui, now);
            crate::stage_display::picker(app, ui);
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
    let audio_pos = crate::audio_ui::position(app);
    let mut new_tops: Option<Vec<f32>> = None;
    let mut new_auto: Option<usize> = None;
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
            let pal = crate::theme::palette(true);
            let next_button = egui::Button::new(
                egui::RichText::new(tr!("Next"))
                    .size(size)
                    .color(pal.on_accent),
            )
            .fill(pal.accent)
            .min_size(egui::vec2(160.0, size * 2.0));
            if ui
                .add(next_button)
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
            let devices: Vec<(bool, String)> = app
                .rig
                .status()
                .into_iter()
                .map(|d| {
                    let ok = d.current.is_some()
                        || matches!(d.state.as_str(), "ready" | "simulated")
                        || d.state.ends_with(" ready");
                    let what = d
                        .current
                        .map(|c| tr!("preset {number}", number = c))
                        .unwrap_or_else(|| crate::i18n::state(&d.state));
                    (
                        ok,
                        format!("{}: {what}{}", d.name, if d.busy { " …" } else { "" }),
                    )
                })
                .collect();
            if let Some(live) = &live_summary {
                ui.separator();
                ui.label(egui::RichText::new(live).size(size * 0.6));
            }
            if !devices.is_empty() {
                ui.separator();
                let pal = crate::theme::pal(ui);
                for (ok, text) in &devices {
                    crate::theme::led(ui, if *ok { pal.ok } else { pal.note }, text);
                    ui.label(egui::RichText::new(text).size(size * 0.6));
                }
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

    // Live session: the band's mark, and a new mark to jump to.
    let cur_song: Option<SongId> = st["current"]["song"].as_str().and_then(|s| s.parse().ok());
    let live_mark = cur_song.and_then(|s| crate::live_ui::band_mark(app, s));
    let live_jump = cur_song.and_then(|s| crate::live_ui::scroll_target(app, s));
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
                    ui.add_space(8.0);
                    if let Some(k) = cur["key"].as_str() {
                        crate::theme::readout(ui, tr!("Key"), k, size * 0.8);
                    }
                    if let Some(t) = cur["tempo_bpm"].as_f64() {
                        crate::theme::readout(ui, "BPM", &format!("{t:.0}"), size * 0.8);
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
                    let highlight = audio_pos
                        .zip(lib.song(id))
                        .and_then(|(pos, s)| section_at(s, &chart.chart, pos));
                    // Follow the backing track: scroll once when it enters a new section.
                    let follow = match highlight {
                        Some(b) if p.auto_block != Some(b) => {
                            p.block_tops.get(b).map(|t| (t - size * 0.5).max(0.0))
                        }
                        _ => None,
                    };
                    // Auto-scroll: move on from wherever the chart is now
                    // (scrolling by hand just moves the starting point).
                    let auto = p.auto_scroll.map(|last| {
                        let dt = (now - last).clamp(0.0, 0.25) as f32;
                        let length = lib.song(id).and_then(|s| s.duration_secs);
                        p.scroll
                            + crate::auto_scroll::speed(
                                p.content_height,
                                p.view_height,
                                length,
                                size,
                                factor,
                            ) * dt
                    });
                    if p.auto_scroll.is_some() {
                        auto_moved = Some(now);
                        ui.ctx().request_repaint();
                    }
                    // Imported from Ultimate Guitar without a chart: one click away.
                    let ug_link = lib.song(id).and_then(|s| {
                        s.links
                            .iter()
                            .find(|l| gigcraft_formats::ultimate_guitar::is_ultimate_guitar(&l.url))
                            .map(|l| l.url.clone())
                    });
                    if chart.chart.blocks.is_empty()
                        && let Some(url) = ug_link
                    {
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(tr!("The chart is on Ultimate Guitar."))
                                .size(size * 0.8)
                                .weak(),
                        );
                        if ui
                            .add(crate::theme::primary(tr!("Open on Ultimate Guitar")))
                            .on_hover_text(url.as_str())
                            .clicked()
                        {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(&url));
                        }
                    }
                    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
                    if let Some(o) = p.pending_scroll.or(live_jump).or(follow).or(auto) {
                        area = area.vertical_scroll_offset(o);
                    }
                    let opts = chart_view::Options {
                        highlight,
                        lyrics_only: false,
                        mark: live_mark.clone(),
                    };
                    let out =
                        area.show(ui, |ui| chart_view::show_with(ui, &chart.chart, size, opts));
                    new_scroll = Some(out.state.clamped_offset().y);
                    view_height = out.inner_rect.height();
                    content_height = out.content_size.y;
                    new_tops = Some(out.inner);
                    new_auto = highlight;
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

    if let (Some(song), Some(tops), Some(offset)) = (cur_song, new_tops.as_ref(), new_scroll) {
        crate::live_ui::chart_shown(app, song, tops, content_height, offset, now);
    }
    if let Some(p) = app.perform.as_mut() {
        p.pending_scroll = None;
        if let Some(t) = new_tops {
            p.block_tops = t;
        }
        p.auto_block = new_auto;
        if let Some(s) = new_scroll {
            p.scroll = s;
        }
        if view_height > 0.0 {
            p.view_height = view_height;
        }
        if content_height > 0.0 {
            p.content_height = content_height;
        }
        if p.auto_scroll.is_some() {
            // At the end of the chart it stops by itself.
            let end = (p.content_height - p.view_height).max(0.0);
            p.auto_scroll = if auto_moved.is_some() && p.scroll < end - 0.5 {
                auto_moved
            } else if auto_moved.is_some() {
                None
            } else {
                p.auto_scroll
            };
        }
    }
    if let Some(up) = speed {
        app.scroll_factor = crate::auto_scroll::step(app.scroll_factor, up);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_track_position_picks_the_section() {
        let chart = gigcraft_chordpro::parse(
            "{start_of_verse: 1}\n[G]a\n{end_of_verse}\n{start_of_chorus}\n[C]b\n{end_of_chorus}\n{start_of_verse: 2}\n[G]c\n{end_of_verse}\n",
            0,
        )
        .chart;
        let labels: Vec<_> = chart.blocks.iter().map(|b| b.label.clone()).collect();
        let mut song = gigcraft_model::Song::new("Test");
        let at = |name: &str, s: f64| gigcraft_model::Marker {
            name: name.into(),
            at_secs: s,
            end_secs: None,
        };
        let verse2 = labels[2].clone().unwrap();
        song.markers = vec![at(" chorus ", 30.0), at(&verse2, 60.0), at("Solo", 90.0)];
        assert_eq!(section_at(&song, &chart, 10.0), None);
        assert_eq!(section_at(&song, &chart, 30.0), Some(1));
        assert_eq!(section_at(&song, &chart, 75.0), Some(2));
        // A marker with no matching section shows nothing.
        assert_eq!(section_at(&song, &chart, 95.0), None);
    }
}
