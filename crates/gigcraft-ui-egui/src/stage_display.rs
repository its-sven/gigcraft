//! A second screen for the stage: a monitor or projector with the current
//! song in very large type – the chart for the band, or lyrics only for
//! singers and the audience. It follows the show and the backing track.

use crate::{GigCraftApp, chart_view};
use gigcraft_model::SongId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Off,
    Chart,
    Lyrics,
}

#[derive(Debug, Default)]
pub(crate) struct StageUi {
    pub(crate) mode: Mode,
    tops: Vec<f32>,
    auto: Option<usize>,
}

/// The "Screen 2" menu (perform mode).
pub(crate) fn picker(app: &mut GigCraftApp, ui: &mut egui::Ui) {
    let label = match app.stage.mode {
        Mode::Off => tr!("Screen 2"),
        Mode::Chart => tr!("Screen 2: chart"),
        Mode::Lyrics => tr!("Screen 2: lyrics"),
    };
    ui.menu_button(label, |ui| {
        for (mode, text) in [
            (Mode::Off, tr!("Off")),
            (Mode::Chart, tr!("Chart with chords (band)")),
            (Mode::Lyrics, tr!("Lyrics only (singers, audience)")),
        ] {
            if ui.selectable_label(app.stage.mode == mode, text).clicked() {
                app.stage.mode = mode;
                ui.close();
            }
        }
    })
    .response
    .on_hover_text(tr!(
        "A second window for a monitor or projector: drag it there and make it full screen"
    ));
}

/// Draw the stage window while it is on.
pub(crate) fn show(app: &mut GigCraftApp, ctx: &egui::Context) {
    if app.stage.mode == Mode::Off {
        return;
    }
    let audio_pos = crate::audio_ui::position(app);
    let builder = egui::ViewportBuilder::default()
        .with_title(tr!("GigCraft – Screen 2"))
        .with_inner_size([1280.0, 720.0]);
    let id = egui::ViewportId::from_hash_of("gigcraft-stage");
    let mut close = false;
    let mut new_tops = None;
    let mut new_auto = None;
    ctx.show_viewport_immediate(id, builder, |ui, _class| {
        if ui.input(|i| i.viewport().close_requested()) {
            close = true;
        }
        let p = crate::theme::palette(true);
        ui.style_mut().visuals = crate::theme::visuals(true);
        let height = ui.ctx().content_rect().height();
        let size = (height / 20.0).clamp(20.0, 72.0);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(32)),
            )
            .show(ui, |ui| {
                ui.style_mut().visuals = crate::theme::visuals(true);
                let Some(st) = app.perform.as_ref().map(|pf| pf.state().clone()) else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("GigCraft")
                                .size(size * 1.5)
                                .color(p.dim),
                        );
                    });
                    return;
                };
                let cur = &st["current"];
                match cur["type"].as_str() {
                    Some("song") => {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(cur["title"].as_str().unwrap_or_default())
                                    .size(size * 1.4)
                                    .strong()
                                    .color(p.text),
                            );
                            if app.stage.mode == Mode::Chart
                                && let Some(k) = cur["key"].as_str()
                            {
                                ui.add_space(12.0);
                                crate::theme::readout(ui, tr!("Key"), k, size * 0.7);
                            }
                        });
                        ui.add_space(size * 0.4);
                        let song: Option<SongId> =
                            cur["song"].as_str().and_then(|s| s.parse().ok());
                        let Some((id, engine)) = song.zip(app.engine.as_ref()) else {
                            return;
                        };
                        let lib = engine.library();
                        let semis = match (
                            lib.song(id).and_then(|s| s.key.clone()),
                            cur["key"].as_str(),
                        ) {
                            (Some(from), Some(to)) => {
                                gigcraft_chordpro::semitones_between(&from, to).unwrap_or(0)
                            }
                            _ => 0,
                        };
                        let chart = app.chart_cache.get(lib, id, semis);
                        let highlight = audio_pos
                            .zip(lib.song(id))
                            .and_then(|(pos, s)| crate::perform::section_at(s, &chart.chart, pos));
                        let follow = match highlight {
                            Some(b) if app.stage.auto != Some(b) => {
                                app.stage.tops.get(b).map(|t| (t - size * 0.3).max(0.0))
                            }
                            _ => None,
                        };
                        let mut area = egui::ScrollArea::vertical()
                            .auto_shrink(false)
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                            );
                        if let Some(o) = follow {
                            area = area.vertical_scroll_offset(o);
                        }
                        let opts = chart_view::Options {
                            highlight,
                            lyrics_only: app.stage.mode == Mode::Lyrics,
                        };
                        let out =
                            area.show(ui, |ui| chart_view::show_with(ui, &chart.chart, size, opts));
                        new_tops = Some(out.inner);
                        new_auto = highlight;
                    }
                    Some("break") => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(height * 0.25);
                            let label = cur["label"]
                                .as_str()
                                .filter(|l| !l.is_empty())
                                .unwrap_or(tr!("Break"));
                            ui.label(
                                egui::RichText::new(label)
                                    .size(size * 2.0)
                                    .strong()
                                    .color(p.text),
                            );
                        });
                    }
                    _ => {}
                }
            });
    });
    if let Some(t) = new_tops {
        app.stage.tops = t;
    }
    app.stage.auto = new_auto;
    if close {
        app.stage.mode = Mode::Off;
    }
}
