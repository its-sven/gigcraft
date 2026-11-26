//! Drawing a chart: chords above lyrics, sections with headings.

use gigcraft_chordpro::{BlockKind, Chart, ChartLine, Parsed};
use gigcraft_model::SongId;
use gigcraft_store::Library;

/// Parsed chart of the selected song, kept until song, transposition or text change.
#[derive(Debug, Default)]
pub(crate) struct Cache {
    key: Option<(SongId, i8, u64)>,
    parsed: Parsed,
}

fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl Cache {
    pub(crate) fn get(&mut self, lib: &Library, id: SongId, transpose: i8) -> Parsed {
        let text = lib.chart(id).unwrap_or_default();
        let key = Some((id, transpose, fingerprint(text)));
        if self.key != key {
            self.parsed = gigcraft_chordpro::parse(text, transpose);
            self.key = key;
        }
        self.parsed.clone()
    }
}

pub(crate) fn show(ui: &mut egui::Ui, chart: &Chart, size: f32) {
    show_with(ui, chart, size, Options::default());
}

/// How to draw a chart.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Options {
    /// Mark this block (the section the backing track is in).
    pub highlight: Option<usize>,
    /// Lyrics without chords (singers, audience screens).
    pub lyrics_only: bool,
}

/// Draw `chart`; returns the top of each block, relative to where the chart starts.
pub(crate) fn show_with(ui: &mut egui::Ui, chart: &Chart, size: f32, opts: Options) -> Vec<f32> {
    let mut tops = Vec::with_capacity(chart.blocks.len());
    if chart.blocks.is_empty() {
        ui.label(egui::RichText::new(tr!("No chart yet.")).weak());
        return tops;
    }
    let origin = ui.cursor().top();
    let chord_color = ui.visuals().hyperlink_color;
    let accent = crate::theme::pal(ui).accent;
    for (bi, block) in chart.blocks.iter().enumerate() {
        let top = ui.cursor().top();
        tops.push(top - origin);
        if let Some(label) = &block.label {
            ui.add_space(size * 0.4);
            let text = egui::RichText::new(label).size(size * 0.8).strong();
            if block.kind == BlockKind::ChorusRecall {
                ui.label(text.italics());
                continue;
            }
            ui.label(text);
        }
        let indent = if block.kind == BlockKind::Chorus {
            size
        } else {
            0.0
        };
        ui.horizontal(|ui| {
            ui.add_space(indent);
            ui.vertical(|ui| {
                for line in &block.lines {
                    match line {
                        ChartLine::Empty => {
                            ui.add_space(size * 0.5);
                        }
                        ChartLine::Comment { text, emphasis } => {
                            let mut t = egui::RichText::new(text).size(size * 0.85).italics();
                            if *emphasis {
                                t = t.strong();
                            }
                            ui.label(t.weak());
                        }
                        ChartLine::Lyrics { segments } if opts.lyrics_only => {
                            let text: String = segments.iter().map(|s| s.text.as_str()).collect();
                            ui.label(egui::RichText::new(text.trim()).size(size));
                        }
                        ChartLine::Lyrics { segments } => {
                            let any_chord = segments.iter().any(|s| s.chord.is_some());
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                                for seg in segments {
                                    ui.vertical(|ui| {
                                        if any_chord {
                                            let chord = seg.chord.as_deref().unwrap_or(" ");
                                            ui.label(
                                                egui::RichText::new(format!("{chord} "))
                                                    .size(size)
                                                    .strong()
                                                    .color(chord_color),
                                            );
                                        }
                                        let text = if seg.text.is_empty() {
                                            " "
                                        } else {
                                            seg.text.as_str()
                                        };
                                        ui.label(egui::RichText::new(text).size(size));
                                    });
                                }
                            });
                        }
                    }
                }
            });
        });
        if opts.highlight == Some(bi) {
            let bottom = ui.cursor().top();
            let x = ui.min_rect().left() - 6.0;
            ui.painter().rect_filled(
                egui::Rect::from_min_max(egui::pos2(x, top), egui::pos2(x + 3.0, bottom)),
                2.0,
                accent,
            );
        }
    }
    tops
}
