//! The rig in the desktop app: MIDI ports, devices, MIDI Learn.

use crate::{GigCraftApp, perform};
use gigcraft_midi::profile;
use gigcraft_perform::Action;
use gigcraft_rig::{DeviceConfig, Level, Mapping, RigConfig, RigOut};

/// MIDI ports of the platform (midir on desktop). Not available on the web.
pub trait MidiIo {
    /// Names of the output ports.
    fn outputs(&mut self) -> Vec<String>;
    /// Open new input ports, close vanished ones; returns the open ones.
    fn refresh_inputs(&mut self) -> Vec<String>;
    /// Send one complete MIDI message.
    fn send(&mut self, port: &str, bytes: &[u8]) -> Result<(), String>;
    /// Messages received since the last call, with their port.
    fn poll(&mut self) -> Vec<(String, Vec<u8>)>;
}

/// Rig state of the app.
#[derive(Debug, Default)]
pub(crate) struct RigUi {
    pub(crate) open: bool,
    last_scan: f64,
    pub(crate) outputs: Vec<String>,
    pub(crate) inputs: Vec<String>,
    /// Which action the next learned trigger gets.
    learning_for: Option<Action>,
    /// Recent device messages (newest last).
    pub(crate) log: Vec<String>,
}

pub(crate) fn now_ms(time: f64) -> u64 {
    (time * 1000.0) as u64
}

/// Called every frame: scan ports, read input, run timers, carry out outputs.
pub(crate) fn pump(app: &mut GigCraftApp, ctx: &egui::Context, time: f64) {
    let now = now_ms(time);
    let mut outs = Vec::new();
    if let Some(io) = app.hooks.midi.as_mut() {
        if time - app.rig_ui.last_scan > 2.0 || app.rig_ui.last_scan == 0.0 {
            app.rig_ui.last_scan = time;
            let outputs = io.outputs();
            app.rig_ui.inputs = io.refresh_inputs();
            if outputs != app.rig_ui.outputs {
                app.rig_ui.outputs = outputs.clone();
                outs.extend(app.rig.ports_changed(&outputs, now));
            }
        }
        for (port, bytes) in io.poll() {
            outs.extend(app.rig.on_input(&port, &bytes, now));
        }
    }
    outs.extend(app.rig.tick(now));
    handle(app, outs, time);
    if let Some(deadline) = app.rig.next_deadline() {
        let wait = deadline.saturating_sub(now).clamp(5, 500);
        ctx.request_repaint_after(std::time::Duration::from_millis(wait));
    } else if app.hooks.midi.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// Carry out what the rig asks for.
pub(crate) fn handle(app: &mut GigCraftApp, outs: Vec<RigOut>, time: f64) {
    for o in outs {
        match o {
            RigOut::Send { port, bytes } => {
                if let Some(io) = app.hooks.midi.as_mut()
                    && let Err(e) = io.send(&port, &bytes)
                {
                    log(app, format!("{port}: {e}"));
                }
            }
            RigOut::Notice {
                device,
                level,
                text,
            } => {
                let line = format!("{device}: {}", crate::i18n::dynamic(&text));
                if matches!(level, Level::Error | Level::Warning) {
                    app.message = Some((line.clone(), true));
                }
                log(app, line);
            }
            RigOut::Action { action } => {
                if app.perform.is_some() {
                    perform::act(app, action, time);
                }
            }
            RigOut::Learned { port, trigger } => {
                if let Some(action) = app.rig_ui.learning_for.take() {
                    let outs = app.rig.add_mapping(Mapping {
                        trigger: trigger.clone(),
                        action,
                        port: None,
                    });
                    log(
                        app,
                        format!("{trigger} @ {port} -> {}", crate::i18n::t(action.label())),
                    );
                    handle(app, outs, time);
                }
            }
            RigOut::ConfigChanged => app.rig_dirty = true,
        }
    }
}

fn log(app: &mut GigCraftApp, line: String) {
    let log = &mut app.rig_ui.log;
    log.push(line);
    if log.len() > 200 {
        log.drain(..log.len() - 200);
    }
}

/// The rig settings window.
pub(crate) fn window(app: &mut GigCraftApp, ctx: &egui::Context, time: f64) {
    if !app.rig_ui.open {
        return;
    }
    let mut open = true;
    let mut new_config: Option<RigConfig> = None;
    let mut learn: Option<Option<Action>> = None;
    let mut test_preset: Option<(String, u8)> = None;
    egui::Window::new(tr!("Rig: MIDI devices and pedals"))
        .open(&mut open)
        .default_width(640.0)
        .show(ctx, |ui| {
            let mut cfg = app.rig.config().clone();
            let status = app.rig.status();
            if app.hooks.midi.is_none() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr!("No MIDI on this platform; only simulated devices work."),
                );
            }
            ui.label(tr!(
                "Devices are matched by the name songs use in their presets (e.g. “Play Acoustic”)."
            ));
            let mut remove = None;
            let mut changed = false;
            for (i, d) in cfg.devices.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(tr!("Name"));
                        changed |= ui
                            .add(egui::TextEdit::singleline(&mut d.name).desired_width(140.0))
                            .lost_focus();
                        ui.label(tr!("Profile"));
                        egui::ComboBox::from_id_salt(("profile", i))
                            .selected_text(profile_name(&d.profile))
                            .show_ui(ui, |ui| {
                                for p in app.rig.profiles() {
                                    changed |= ui
                                        .selectable_value(&mut d.profile, p.id.clone(), &p.name)
                                        .changed();
                                }
                            });
                        if ui.small_button(tr!("Remove")).clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        changed |= ui
                            .checkbox(&mut d.simulated, tr!("Simulated (no hardware)"))
                            .changed();
                        if !d.simulated {
                            ui.label(tr!("Port"));
                            let label = d.port.clone().unwrap_or_else(|| tr!("automatic").into());
                            egui::ComboBox::from_id_salt(("port", i))
                                .selected_text(label)
                                .show_ui(ui, |ui| {
                                    changed |= ui
                                        .selectable_value(&mut d.port, None, tr!("automatic"))
                                        .changed();
                                    for p in &app.rig_ui.outputs {
                                        changed |= ui
                                            .selectable_value(&mut d.port, Some(p.clone()), p)
                                            .changed();
                                    }
                                });
                        }
                    });
                    if let Some(st) = status.iter().find(|s| s.name == d.name) {
                        ui.horizontal(|ui| {
                            ui.small(format!(
                                "{}{}{}",
                                crate::i18n::state(&st.state),
                                st.port
                                    .as_ref()
                                    .map(|p| format!(" · {p}"))
                                    .unwrap_or_default(),
                                st.current
                                    .as_ref()
                                    .map(|c| format!(" · {}", tr!("preset {number}", number = c)))
                                    .unwrap_or_default()
                            ));
                            if ui.small_button(tr!("Test: program 0")).clicked() {
                                test_preset = Some((d.name.clone(), 0));
                            }
                            if ui.small_button(tr!("program 1")).clicked() {
                                test_preset = Some((d.name.clone(), 1));
                            }
                        });
                    }
                });
            }
            if let Some(i) = remove {
                cfg.devices.remove(i);
                changed = true;
            }
            ui.horizontal(|ui| {
                if ui.button(tr!("Add device")).clicked() {
                    cfg.devices.push(DeviceConfig {
                        name: tr!("Device {number}", number = cfg.devices.len() + 1),
                        port: None,
                        profile: profile::GENERIC_MIDI.into(),
                        simulated: false,
                        program_change: None,
                        pc_offset: None,
                    });
                    changed = true;
                }
                if ui.button(tr!("Add simulated Play Acoustic")).clicked() {
                    cfg.devices.extend(RigConfig::demo().devices);
                    changed = true;
                }
            });
            ui.separator();
            ui.strong(tr!("MIDI footswitches and controllers"));
            ui.small(tr!(
                "Click Learn, then press the switch or move the controller."
            ));
            let mut remove_map = None;
            for (i, m) in cfg.mappings.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{}  →  {}",
                        m.trigger,
                        crate::i18n::t(m.action.label())
                    ));
                    if ui.small_button(tr!("Remove")).clicked() {
                        remove_map = Some(i);
                    }
                });
            }
            if let Some(i) = remove_map {
                cfg.mappings.remove(i);
                changed = true;
            }
            ui.horizontal_wrapped(|ui| {
                for a in Action::ALL {
                    let waiting = app.rig_ui.learning_for == Some(a);
                    let label = crate::i18n::t(a.label());
                    let text = if waiting {
                        tr!("Waiting… ({action})", action = label)
                    } else {
                        tr!("Learn {action}", action = label)
                    };
                    if ui.selectable_label(waiting, text).clicked() {
                        learn = Some(if waiting { None } else { Some(a) });
                    }
                }
            });
            ui.label(tr!(
                "MIDI inputs: {list}",
                list = if app.rig_ui.inputs.is_empty() {
                    tr!("none").to_string()
                } else {
                    app.rig_ui.inputs.join(", ")
                }
            ));
            ui.separator();
            egui::CollapsingHeader::new(tr!("Log")).show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &app.rig_ui.log {
                            ui.small(line);
                        }
                    });
            });
            if changed {
                new_config = Some(cfg);
            }
        });
    app.rig_ui.open = open;
    let now = now_ms(time);
    if let Some(cfg) = new_config {
        let outs = app.rig.set_config(cfg, now);
        let outputs = app.rig_ui.outputs.clone();
        let more = app.rig.ports_changed(&outputs, now);
        app.rig_dirty = true;
        handle(app, outs, time);
        handle(app, more, time);
    }
    if let Some(l) = learn {
        app.rig_ui.learning_for = l;
        app.rig.learn(l.is_some());
    }
    if let Some((device, program)) = test_preset {
        let action = gigcraft_model::MidiAction {
            device,
            message: gigcraft_model::MidiMessage::ProgramChange {
                channel: 1,
                program,
                bank_msb: None,
                bank_lsb: None,
            },
        };
        let outs = app.rig.send_part(&[action], now);
        handle(app, outs, time);
    }
}

fn profile_name(id: &str) -> String {
    profile::builtin_by_id(id).map_or_else(|| id.to_string(), |p| p.name)
}
