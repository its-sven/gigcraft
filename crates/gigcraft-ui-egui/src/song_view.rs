//! Song view and song editor.

use crate::{GigCraftApp, chart_view};
use gigcraft_model::{
    ClickSettings, Marker, MidiAction, MidiMessage, Part, SongId, Track, TrackRole, format_duration,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A program change shown as editable fields.
#[derive(Debug, Clone, Default)]
struct PresetDraft {
    device: String,
    channel: String,
    program: String,
}

#[derive(Debug, Clone, Default)]
struct PartDraft {
    name: String,
    notes: String,
    presets: Vec<PresetDraft>,
    /// Actions this editor does not edit (CC, SysEx…), kept unchanged.
    other: Vec<MidiAction>,
}

/// The song being edited, as text fields.
#[derive(Debug, Clone)]
pub(crate) struct Draft {
    id: SongId,
    title: String,
    artist: String,
    key: String,
    tempo: String,
    time: String,
    duration: String,
    capo: String,
    tags: String,
    notes: String,
    chart: String,
    parts: Vec<PartDraft>,
    /// (label, address) per link.
    links: Vec<(String, String)>,
    tracks: Vec<Track>,
    click_on: bool,
    click: ClickSettings,
    /// Audio attachments that can become tracks.
    audio_files: Vec<String>,
    /// Start time ("m:ss") per chart section label, for following the track.
    section_times: BTreeMap<String, String>,
    /// Markers that are not section starts, kept as they are.
    other_markers: Vec<Marker>,
    issues: Vec<String>,
}

impl Draft {
    pub(crate) fn load(app: &GigCraftApp, id: SongId) -> Option<Self> {
        let lib = app.engine.as_ref()?.library();
        let s = lib.song(id)?;
        let parts = s
            .parts
            .iter()
            .map(|p| {
                let mut d = PartDraft {
                    name: p.name.clone(),
                    notes: p.notes.clone(),
                    ..Default::default()
                };
                for a in &p.actions {
                    match &a.message {
                        MidiMessage::ProgramChange {
                            channel,
                            program,
                            bank_msb: None,
                            bank_lsb: None,
                        } => {
                            d.presets.push(PresetDraft {
                                device: a.device.clone(),
                                channel: channel.to_string(),
                                program: program.to_string(),
                            });
                        }
                        _ => d.other.push(a.clone()),
                    }
                }
                d
            })
            .collect();
        Some(Self {
            id,
            title: s.title.clone(),
            artist: s.artist.clone(),
            key: s.key.clone().unwrap_or_default(),
            tempo: s.tempo_bpm.map(|t| format!("{t}")).unwrap_or_default(),
            time: s.time_signature.clone().unwrap_or_default(),
            duration: s.duration_secs.map(format_duration).unwrap_or_default(),
            capo: s.capo.map(|c| c.to_string()).unwrap_or_default(),
            tags: s.tags.join(", "),
            notes: s.notes.clone(),
            chart: lib.chart(id).unwrap_or_default().to_string(),
            parts,
            links: s
                .links
                .iter()
                .map(|l| (l.label.clone().unwrap_or_default(), l.url.clone()))
                .collect(),
            tracks: s.tracks.clone(),
            click_on: s.click.as_ref().is_some_and(|c| c.enabled),
            click: s.click.clone().unwrap_or_default(),
            section_times: s
                .markers
                .iter()
                .map(|m| (m.name.clone(), clock_text(m.at_secs)))
                .collect(),
            other_markers: Vec::new(),
            audio_files: s
                .attachments
                .iter()
                .filter(|a| a.kind() == gigcraft_model::AttachmentKind::Audio)
                .map(|a| a.file.clone())
                .collect(),
            issues: Vec::new(),
        })
    }

    /// The fields for `song.update`, or a message about what is wrong.
    fn fields(&self) -> Result<Value, String> {
        let opt = |s: &str| {
            if s.trim().is_empty() {
                Value::Null
            } else {
                json!(s.trim())
            }
        };
        let tempo = match self.tempo.trim() {
            "" => Value::Null,
            t => json!(
                t.replace(',', ".")
                    .parse::<f64>()
                    .map_err(|_| tr!("Tempo “{value}” is not a number", value = t))?
            ),
        };
        let capo = match self.capo.trim() {
            "" => Value::Null,
            c => json!(
                c.parse::<u8>()
                    .map_err(|_| tr!("Capo “{value}” is not a number", value = c))?
            ),
        };
        let mut links = Vec::new();
        for (label, url) in &self.links {
            let url = url.trim();
            if url.is_empty() {
                continue;
            }
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(tr!(
                    "The link “{value}” must start with https:// or http://",
                    value = url
                ));
            }
            let label = label.trim();
            links.push(json!({
                "url": url,
                "label": if label.is_empty() { Value::Null } else { json!(label) },
            }));
        }
        let mut parts = Vec::new();
        for p in &self.parts {
            let mut actions = Vec::new();
            for pr in &p.presets {
                let channel: u8 = pr
                    .channel
                    .trim()
                    .parse()
                    .map_err(|_| tr!("{part}: channel must be 1–16", part = p.name))?;
                let program: u8 = pr
                    .program
                    .trim()
                    .parse()
                    .map_err(|_| tr!("{part}: program must be 0–127", part = p.name))?;
                actions.push(MidiAction {
                    device: pr.device.trim().to_string(),
                    message: MidiMessage::ProgramChange {
                        channel,
                        program,
                        bank_msb: None,
                        bank_lsb: None,
                    },
                });
            }
            actions.extend(p.other.iter().cloned());
            parts.push(Part {
                name: p.name.trim().to_string(),
                actions,
                notes: p.notes.clone(),
            });
        }
        let tags: Vec<String> = self
            .tags
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect();
        Ok(json!({
            "title": self.title.trim(),
            "artist": self.artist.trim(),
            "key": opt(&self.key),
            "tempo_bpm": tempo,
            "time_signature": opt(&self.time),
            "duration": opt(&self.duration),
            "capo": capo,
            "tags": tags,
            "notes": self.notes,
            "parts": serde_json::to_value(parts).unwrap_or(Value::Null),
            "links": links,
            "tracks": serde_json::to_value(&self.tracks).unwrap_or(Value::Null),
            "markers": serde_json::to_value(self.markers()?).unwrap_or(Value::Null),
            "click": if self.click_on {
                serde_json::to_value(ClickSettings { enabled: true, ..self.click.clone() }).unwrap_or(Value::Null)
            } else {
                Value::Null
            },
        }))
    }
}

pub(crate) fn view(app: &mut GigCraftApp, ui: &mut egui::Ui, id: SongId) {
    let Some(engine) = &app.engine else { return };
    let lib = engine.library();
    let Some(song) = lib.song(id).cloned() else {
        ui.label(tr!("This song is no longer in the library."));
        return;
    };
    let chart = app.chart_cache.get(lib, id, app.transpose);
    let size = app.font_size;
    let mut edit = false;
    let mut trash = false;
    let mut scroll_toggle = false;
    let mut scroll_speed: Option<bool> = None;
    let now = ui.input(|i| i.time);
    let length = song.duration_secs;
    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new(&song.title).size(size * 1.4));
        if !song.artist.is_empty() {
            ui.label(egui::RichText::new(&song.artist).size(size));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            trash = ui
                .button(tr!("Move to trash"))
                .on_hover_text(tr!(
                    "Move this song to the library's trash; Undo brings it back"
                ))
                .clicked();
            edit = ui
                .button(tr!("Edit"))
                .on_hover_text(tr!("Change fields, parts, presets and the chart"))
                .clicked();
            ui.separator();
            // Auto-scroll for practising (right to left: faster, speed, slower, on/off).
            if crate::named(ui.small_button("+"), tr!("Scroll faster"))
                .on_hover_text(tr!("Scroll faster"))
                .clicked()
            {
                scroll_speed = Some(true);
            }
            crate::theme::readout(ui, tr!("Speed"), &format!("{:.0}%", app.scroll_factor * 100.0), 13.0);
            if crate::named(ui.small_button("−"), tr!("Scroll slower"))
                .on_hover_text(tr!("Scroll slower"))
                .clicked()
            {
                scroll_speed = Some(false);
            }
            let running = app.song_scroll.running.is_some();
            let label = if running { tr!("Stop scrolling") } else { tr!("Auto-scroll") };
            let button = if running { crate::theme::primary(label) } else { egui::Button::new(label) };
            scroll_toggle = crate::named(ui.add(button), tr!("Auto-scroll"))
                .on_hover_text(tr!("The chart scrolls by itself, as fast as the song is long. Scroll by hand at any time."))
                .clicked();
        });
    });
    ui.horizontal(|ui| {
        let mut facts: Vec<(&str, String)> = Vec::new();
        if let Some(k) = chart.chart.key.clone().or_else(|| song.key.clone()) {
            facts.push((tr!("Key"), k));
        }
        if let Some(t) = song.tempo_bpm {
            facts.push(("BPM", format!("{t:.0}")));
        }
        if let Some(ts) = &song.time_signature {
            facts.push((tr!("Time"), ts.clone()));
        }
        if let Some(c) = song.capo.filter(|c| *c > 0) {
            facts.push((tr!("Capo"), c.to_string()));
        }
        if let Some(t) = song.custom.get("Tuning") {
            facts.push((tr!("Tuning"), t.clone()));
        }
        if let Some(d) = song.duration_secs {
            facts.push((tr!("Length"), format_duration(d)));
        }
        for (label, value) in &facts {
            crate::theme::readout(ui, label, value, 15.0);
        }
        ui.separator();
        if crate::named(ui.button("−"), tr!("Transpose down"))
            .on_hover_text(tr!("Transpose down"))
            .clicked()
        {
            app.transpose = if app.transpose <= -11 {
                0
            } else {
                app.transpose - 1
            };
        }
        crate::theme::readout(
            ui,
            tr!("Transpose"),
            &if app.transpose == 0 {
                "±0".to_string()
            } else {
                format!("{:+}", app.transpose)
            },
            15.0,
        );
        if crate::named(ui.button("+"), tr!("Transpose up"))
            .on_hover_text(tr!("Transpose up"))
            .clicked()
        {
            app.transpose = if app.transpose >= 11 {
                0
            } else {
                app.transpose + 1
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
    if !song.links.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for l in &song.links {
                let label = l.label.clone().unwrap_or_else(|| l.url.clone());
                let r = ui
                    .button(tr!("Open {label}", label = label))
                    .on_hover_text(&l.url);
                if r.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(&l.url));
                }
                r.context_menu(|ui| {
                    if ui.button(tr!("Copy link")).clicked() {
                        ui.ctx().copy_text(l.url.clone());
                        ui.close();
                    }
                });
            }
        });
    }
    let mut attach = false;
    let mut detach = None;
    let attachments: Vec<(String, std::path::PathBuf)> = song
        .attachments
        .iter()
        .filter_map(|a| {
            Some((
                a.label.clone().unwrap_or_else(|| a.file.clone()),
                lib.attachment_path(id, &a.file)?,
            ))
        })
        .collect();
    ui.horizontal_wrapped(|ui| {
        for ((label, path), a) in attachments.iter().zip(&song.attachments) {
            let r = ui
                .button(format!("{} {label}", kind_label(a.kind())))
                .on_hover_text(tr!("Open; right-click to remove"));
            if r.clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(format!("file://{}", path.display())));
            }
            r.context_menu(|ui| {
                if ui.button(tr!("Remove attachment")).clicked() {
                    detach = Some(a.file.clone());
                    ui.close();
                }
            });
        }
        if app.hooks.pick_file.is_some()
            && ui
                .small_button(tr!("Attach file…"))
                .on_hover_text(tr!("PDF chart, image, recording – copied into the library"))
                .clicked()
        {
            attach = true;
        }
    });
    ui.separator();
    for w in &chart.warnings {
        ui.colored_label(ui.visuals().warn_fg_color, w);
    }
    let mut area = egui::ScrollArea::vertical().auto_shrink(false);
    if let Some(o) = app
        .song_scroll
        .next_offset(now, length, size, app.scroll_factor)
    {
        area = area.vertical_scroll_offset(o);
        ui.ctx().request_repaint();
    }
    let out = area.show(ui, |ui| {
        chart_view::show(ui, &chart.chart, size);
    });
    app.song_scroll.after(
        now,
        out.state.clamped_offset().y,
        out.inner_rect.height(),
        out.content_size.y,
    );
    if scroll_toggle {
        app.song_scroll.toggle(now);
    }
    if let Some(up) = scroll_speed {
        app.scroll_factor = crate::auto_scroll::step(app.scroll_factor, up);
    }
    if edit {
        app.song_draft = Draft::load(app, id);
    }
    if attach
        && let Some(path) = app.hooks.pick_file.as_ref().and_then(|pick| pick())
        && app
            .run(
                "song.attach",
                json!({"id": id.to_string(), "path": path.display().to_string()}),
            )
            .is_some()
    {
        app.notify(tr!("File attached."));
    }
    if let Some(file) = detach {
        app.run("song.detach", json!({"id": id.to_string(), "file": file}));
    }
    if trash
        && app
            .run("song.trash", json!({"id": id.to_string()}))
            .is_some()
    {
        app.song = None;
        app.notify(tr!(
            "“{title}” moved to the library's trash (Undo brings it back).",
            title = song.title
        ));
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &mut String, width: f32) {
    ui.label(label);
    ui.add_sized(
        [width, ui.spacing().interact_size.y],
        egui::TextEdit::singleline(value),
    );
}

pub(crate) fn editor(app: &mut GigCraftApp, ui: &mut egui::Ui, id: SongId) {
    let Some(mut draft) = app.song_draft.take() else {
        return;
    };
    if draft.id != id {
        app.song_draft = Draft::load(app, id);
        return;
    }
    let mut save = false;
    let mut cancel = false;
    ui.horizontal(|ui| {
        ui.heading(tr!("Edit song"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel = ui
                .button(tr!("Cancel"))
                .on_hover_text(tr!("Discard the changes"))
                .clicked();
            save = ui
                .add(crate::theme::primary(tr!("Save")))
                .on_hover_text(tr!("Save the changes (Cmd+S or Ctrl+S)"))
                .clicked();
        });
    });
    let save_key = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S));
    save |= save_key;
    for issue in &draft.issues {
        ui.colored_label(ui.visuals().warn_fg_color, issue);
    }
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        egui::Grid::new("song-fields").num_columns(4).spacing([12.0, 6.0]).show(ui, |ui| {
            field(ui, tr!("Title"), &mut draft.title, 280.0);
            field(ui, tr!("Artist"), &mut draft.artist, 220.0);
            ui.end_row();
            field(ui, tr!("Key"), &mut draft.key, 60.0);
            field(ui, tr!("Tempo (BPM)"), &mut draft.tempo, 60.0);
            ui.end_row();
            field(ui, tr!("Time"), &mut draft.time, 60.0);
            field(ui, tr!("Duration (m:ss)"), &mut draft.duration, 60.0);
            ui.end_row();
            field(ui, tr!("Capo"), &mut draft.capo, 60.0);
            field(ui, tr!("Tags (comma)"), &mut draft.tags, 220.0);
            ui.end_row();
        });
        ui.label(tr!("Notes"));
        ui.add(egui::TextEdit::multiline(&mut draft.notes).desired_rows(2).desired_width(f32::INFINITY));

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            crate::theme::section(ui, tr!("Links"));
            if ui
                .small_button(tr!("Add link"))
                .on_hover_text(tr!("A web page for this song: a tab, a video, the original recording"))
                .clicked()
            {
                draft.links.push((String::new(), String::new()));
            }
        });
        let mut remove_link = None;
        for (i, (label, url)) in draft.links.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(label).desired_width(180.0).hint_text(tr!("Label")));
                ui.add(egui::TextEdit::singleline(url).desired_width(380.0).hint_text("https://…"));
                if ui.small_button(tr!("Remove")).on_hover_text(tr!("Remove this link")).clicked() {
                    remove_link = Some(i);
                }
            });
        }
        if let Some(i) = remove_link {
            draft.links.remove(i);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            crate::theme::section(ui, tr!("Parts and presets"));
            if ui.small_button(tr!("Add part")).on_hover_text(tr!("A part (intro, verse, solo…) can switch presets when it starts")).clicked() {
                let name = tr!("Part {number}", number = draft.parts.len() + 1);
                draft.parts.push(PartDraft { name, ..Default::default() });
            }
        });
        ui.small(tr!("Each part sends its presets when it starts. Program numbers are MIDI values 0–127."));
        let mut remove_part = None;
        let mut move_up = None;
        for (pi, part) in draft.parts.iter_mut().enumerate() {
            crate::theme::card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut part.name).desired_width(140.0).hint_text(tr!("Part name")));
                    ui.add(egui::TextEdit::singleline(&mut part.notes).desired_width(220.0).hint_text(tr!("Notes")));
                    if pi > 0 && ui.small_button(tr!("Up")).on_hover_text(tr!("Move this part up")).clicked() {
                        move_up = Some(pi);
                    }
                    if ui.small_button(tr!("Remove part")).on_hover_text(tr!("Delete this part")).clicked() {
                        remove_part = Some(pi);
                    }
                });
                let mut remove_preset = None;
                for (ri, pr) in part.presets.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(tr!("Device"));
                        ui.add(egui::TextEdit::singleline(&mut pr.device).desired_width(140.0));
                        ui.label(tr!("Channel"));
                        ui.add(egui::TextEdit::singleline(&mut pr.channel).desired_width(30.0));
                        ui.label(tr!("Program"));
                        ui.add(egui::TextEdit::singleline(&mut pr.program).desired_width(40.0));
                        if ui.small_button(tr!("Remove")).on_hover_text(tr!("Remove this program change")).clicked() {
                            remove_preset = Some(ri);
                        }
                    });
                }
                if let Some(ri) = remove_preset {
                    part.presets.remove(ri);
                }
                if !part.other.is_empty() {
                    ui.small(trn!(
                        part.other.len(),
                        "+ {n} other MIDI message, kept as it is",
                        "+ {n} other MIDI messages, kept as they are"
                    ));
                }
                if ui.small_button(tr!("Add program change")).on_hover_text(tr!("Send a preset to a device when this part starts")).clicked() {
                    let device = part.presets.last().map(|p| p.device.clone()).unwrap_or_else(|| "Play Acoustic".into());
                    part.presets.push(PresetDraft { device, channel: "1".into(), program: "0".into() });
                }
            });
        }
        if let Some(pi) = remove_part {
            draft.parts.remove(pi);
        }
        if let Some(pi) = move_up {
            draft.parts.swap(pi - 1, pi);
        }

        ui.add_space(8.0);
        tracks_editor(ui, &mut draft);

        ui.add_space(8.0);
        crate::theme::section(ui, tr!("Chart (ChordPro)"));
        ui.small(tr!("Chords in [brackets] before the syllable; sections with {start_of_chorus} … {end_of_chorus}; {comment: …}."));
        ui.columns(2, |cols| {
            cols[0].add(
                egui::TextEdit::multiline(&mut draft.chart)
                    .code_editor()
                    .desired_rows(24)
                    .desired_width(f32::INFINITY),
            );
            let parsed = gigcraft_chordpro::parse(&draft.chart, 0);
            for w in &parsed.warnings {
                cols[1].colored_label(cols[1].visuals().warn_fg_color, w);
            }
            chart_view::show(&mut cols[1], &parsed.chart, (app.font_size * 0.8).max(12.0));
        });
    });

    if cancel {
        return;
    }
    if save {
        match draft.fields() {
            Ok(fields) => {
                let chart = draft.chart.clone();
                if let Some(out) = app.run(
                    "song.update",
                    json!({"id": id.to_string(), "fields": fields, "chart": chart}),
                ) {
                    let issues: Vec<String> = out.result["issues"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|i| {
                            format!(
                                "{}: {}",
                                i["field"].as_str().unwrap_or(""),
                                i["message"].as_str().unwrap_or("")
                            )
                        })
                        .collect();
                    if issues.is_empty() {
                        app.notify(tr!("Saved."));
                        return;
                    }
                    draft.issues = issues;
                } else if app.last_error == Some("conflict") {
                    // Someone else changed this song: load their version, keep ours in the editor.
                    app.run("library.reload", Value::Null);
                    draft.issues = vec![
                        tr!("This song was changed on another device. The library has been reloaded; save again to replace that version with yours, or cancel to keep it.")
                            .into(),
                    ];
                }
            }
            Err(msg) => draft.issues = vec![msg],
        }
    }
    app.song_draft = Some(draft);
}

fn kind_label(kind: gigcraft_model::AttachmentKind) -> &'static str {
    use gigcraft_model::AttachmentKind as K;
    match kind {
        K::Pdf => "PDF",
        K::Image => tr!("Image"),
        K::Audio => tr!("Audio"),
        K::Midi => "MIDI",
        K::Text => tr!("Text"),
        K::Other => tr!("File"),
    }
}

/// "1:23" for 83.0 s, "1:23.5" for 83.5 s.
fn clock_text(secs: f64) -> String {
    let whole = secs.max(0.0);
    let m = (whole / 60.0).floor();
    let s = whole - m * 60.0;
    if (s - s.round()).abs() < 0.05 {
        format!("{}:{:02}", m as u64, s.round() as u64)
    } else {
        format!("{}:{:04.1}", m as u64, s)
    }
}

/// Seconds from "1:23", "1:23.5" or "83".
fn parse_clock(text: &str) -> Option<f64> {
    let t = text.trim().replace(',', ".");
    let (m, s) = match t.split_once(':') {
        Some((m, s)) => (m.trim().parse::<f64>().ok()?, s.trim().parse::<f64>().ok()?),
        None => (0.0, t.parse::<f64>().ok()?),
    };
    let v = m * 60.0 + s;
    let ok =
        v.is_finite() && v >= 0.0 && m >= 0.0 && (!t.contains(':') || (0.0..60.0).contains(&s));
    ok.then_some(v)
}

impl Draft {
    /// Section start times as markers, plus the markers this editor does not show.
    fn markers(&self) -> Result<Vec<Marker>, String> {
        let mut out = self.other_markers.clone();
        for (label, text) in &self.section_times {
            if text.trim().is_empty() {
                continue;
            }
            let at = parse_clock(text).ok_or_else(|| {
                tr!(
                    "{section}: “{value}” is not a time like 1:23",
                    section = label,
                    value = text
                )
            })?;
            out.push(Marker {
                name: label.clone(),
                at_secs: at,
                end_secs: None,
            });
        }
        out.sort_by(|a, b| a.at_secs.total_cmp(&b.at_secs));
        Ok(out)
    }
}

/// Output pair of a bus: bus 0 → "1/2", bus 1 → "3/4".
fn bus_label(bus: u32) -> String {
    format!("{}/{}", bus * 2 + 1, bus * 2 + 2)
}

fn bus_picker(ui: &mut egui::Ui, salt: impl std::hash::Hash + std::fmt::Debug, bus: &mut u32) {
    egui::ComboBox::from_id_salt(salt)
        .width(60.0)
        .selected_text(bus_label(*bus))
        .show_ui(ui, |ui| {
            for b in 0..8 {
                ui.selectable_value(bus, b, bus_label(b));
            }
        })
        .response
        .on_hover_text(tr!("Audio outputs: 1/2 is the main pair; 3/4 and up need an interface with more outputs (e.g. click to in-ears only)"));
}

fn role_label(role: TrackRole) -> &'static str {
    match role {
        TrackRole::Backing => tr!("Backing"),
        TrackRole::Click => tr!("Click track"),
        TrackRole::Guide => tr!("Guide / cues"),
        TrackRole::Stem => tr!("Stem"),
    }
}

/// Backing tracks and the click.
fn tracks_editor(ui: &mut egui::Ui, draft: &mut Draft) {
    ui.horizontal(|ui| {
        crate::theme::section(ui, tr!("Backing tracks and click"));
        let unused: Vec<String> = draft
            .audio_files
            .iter()
            .filter(|f| !draft.tracks.iter().any(|t| &t.file == *f))
            .cloned()
            .collect();
        ui.add_enabled_ui(!unused.is_empty(), |ui| {
            ui.menu_button(tr!("Add track"), |ui| {
                for f in &unused {
                    if ui.button(f).clicked() {
                        draft.tracks.push(Track::new(f.clone()));
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text(tr!(
                "Play one of this song's audio attachments in perform mode"
            ))
            .on_disabled_hover_text(tr!(
                "Attach an audio file first (Attach file… in the song view)"
            ));
        });
    });
    ui.small(tr!("In perform mode, Play starts the tracks and the click together; the next song's files are loaded in the background."));
    let mut remove = None;
    for (i, t) in draft.tracks.iter_mut().enumerate() {
        crate::theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(&t.file).strong());
                egui::ComboBox::from_id_salt(("track-role", i))
                    .selected_text(role_label(t.role))
                    .show_ui(ui, |ui| {
                        for r in [
                            TrackRole::Backing,
                            TrackRole::Click,
                            TrackRole::Guide,
                            TrackRole::Stem,
                        ] {
                            ui.selectable_value(&mut t.role, r, role_label(r));
                        }
                    })
                    .response
                    .on_hover_text(tr!(
                        "What this track is; guide and click tracks usually go to the in-ears only"
                    ));
                ui.label(tr!("Volume"));
                ui.add(
                    egui::Slider::new(&mut t.gain_db, -40.0..=12.0)
                        .suffix(" dB")
                        .fixed_decimals(1),
                )
                .on_hover_text(tr!(
                    "Track volume in decibels; 0 dB plays the file as it is"
                ));
                ui.label(tr!("Pan"));
                ui.add(
                    egui::Slider::new(&mut t.pan, -1.0..=1.0)
                        .fixed_decimals(2)
                        .show_value(false),
                )
                .on_hover_text(tr!("Left – centre – right"));
                ui.label(tr!("Out"));
                bus_picker(ui, ("track-bus", i), &mut t.bus);
                ui.checkbox(&mut t.mute, tr!("Mute"))
                    .on_hover_text(tr!("Keep the track but do not play it"));
                if ui
                    .small_button(tr!("Remove"))
                    .on_hover_text(tr!("Remove the track (the file stays attached)"))
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        draft.tracks.remove(i);
    }
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut draft.click_on, tr!("Click"))
            .on_hover_text(tr!(
                "A metronome click at the song's tempo, generated by GigCraft"
            ));
        ui.add_enabled_ui(draft.click_on, |ui| {
            ui.label(tr!("Count-in (bars)"));
            ui.add(egui::DragValue::new(&mut draft.click.count_in_bars).range(0..=4))
                .on_hover_text(tr!("Bars of click before the tracks start"));
            ui.label(tr!("Volume"));
            ui.add(
                egui::Slider::new(&mut draft.click.volume_db, -40.0..=12.0)
                    .suffix(" dB")
                    .fixed_decimals(1),
            );
            ui.label(tr!("Out"));
            bus_picker(ui, "click-bus", &mut draft.click.bus);
        });
        if draft.click_on && draft.tempo.trim().is_empty() {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                tr!("The click needs a tempo (BPM) above."),
            );
        }
    });
    if draft.tracks.is_empty() && !draft.click_on {
        return;
    }
    // Section start times: perform mode follows the backing track through the chart.
    let labels: Vec<String> = gigcraft_chordpro::parse(&draft.chart, 0)
        .chart
        .blocks
        .iter()
        .filter(|b| b.kind != gigcraft_chordpro::BlockKind::ChorusRecall)
        .filter_map(|b| b.label.clone())
        .fold(Vec::new(), |mut v, l| {
            if !v.contains(&l) {
                v.push(l);
            }
            v
        });
    if labels.is_empty() {
        return;
    }
    ui.add_space(4.0);
    ui.label(tr!("Section start times"))
        .on_hover_text(tr!("When the backing track reaches a section, perform mode scrolls to it and marks it. Leave empty for sections you do not need."));
    egui::Grid::new("section-times")
        .num_columns(4)
        .spacing([10.0, 4.0])
        .show(ui, |ui| {
            for (i, label) in labels.iter().enumerate() {
                ui.label(label);
                let text = draft.section_times.entry(label.clone()).or_default();
                ui.add(
                    egui::TextEdit::singleline(text)
                        .hint_text("m:ss")
                        .desired_width(60.0),
                );
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
    // Markers whose names are no longer chart sections stay untouched.
    let keep: Vec<String> = draft
        .section_times
        .keys()
        .filter(|k| !labels.contains(k))
        .cloned()
        .collect();
    for k in keep {
        if let Some(text) = draft.section_times.remove(&k)
            && let Some(at) = parse_clock(&text)
        {
            draft.other_markers.push(Marker {
                name: k,
                at_secs: at,
                end_secs: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_times_round_trip() {
        assert_eq!(parse_clock("1:23"), Some(83.0));
        assert_eq!(parse_clock("1:23,5"), Some(83.5));
        assert_eq!(parse_clock(" 83 "), Some(83.0));
        assert_eq!(parse_clock("0:00"), Some(0.0));
        assert_eq!(parse_clock("1:75"), None);
        assert_eq!(parse_clock("-5"), None);
        assert_eq!(parse_clock("x"), None);
        assert_eq!(clock_text(83.0), "1:23");
        assert_eq!(clock_text(83.5), "1:23.5");
        assert_eq!(clock_text(207.4), "3:27.4");
    }
}
