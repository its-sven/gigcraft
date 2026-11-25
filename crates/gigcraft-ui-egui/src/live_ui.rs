//! Band on stage: lead a live session (web viewer, QR code, roster, cues) or
//! follow one (own chart and own presets for the leader's current song).
//!
//! The network is behind [`LiveIo`], provided by the shell (desktop: the
//! `gigcraft-live-net` WebSocket server and client), so this crate still
//! builds for the web.

use crate::{GigCraftApp, chart_view};
use gigcraft_live::{
    ConnId, Cue, Follower, FollowerEvent, Leader, LeaderEvent, LeaderOut, Slot, Snapshot,
};
use serde_json::Value;

/// Network events of a hosted session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvent {
    Connected { conn: u64, peer: String },
    Text { conn: u64, text: String },
    Closed { conn: u64 },
}

/// Network events of a joined session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinEvent {
    Connected,
    Text(String),
    Closed(String),
}

/// A started host: where band members find it, and a unique session id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hosted {
    pub urls: Vec<String>,
    pub session: String,
}

/// The local network, provided by the shell.
pub trait LiveIo {
    /// Start serving the web viewer and the live WebSocket.
    fn host(&mut self) -> Result<Hosted, String>;
    fn stop_host(&mut self);
    fn poll_host(&mut self) -> Vec<HostEvent>;
    fn send(&mut self, conn: u64, text: String);
    fn close(&mut self, conn: u64);
    /// Connect to a leader (`192.168.1.20:8787` or the viewer address).
    fn join(&mut self, address: &str);
    fn leave(&mut self);
    fn poll_join(&mut self) -> Vec<JoinEvent>;
    fn send_to_leader(&mut self, text: String);
    /// Dark/light modules of a QR code for `text`.
    fn qr(&self, text: &str) -> Option<Vec<Vec<bool>>>;
}

#[derive(Debug, Default)]
pub(crate) enum Mode {
    #[default]
    Off,
    Leading {
        leader: Leader,
        urls: Vec<String>,
        /// (setlist, index, part, running) last published.
        published: Option<String>,
    },
    Following {
        follower: Follower,
        address: String,
        connected: bool,
        status: String,
    },
}

/// Live state of the app.
#[derive(Debug, Default)]
pub(crate) struct LiveUi {
    pub(crate) open: bool,
    pub(crate) mode: Mode,
    pub(crate) name: String,
    pub(crate) instrument: String,
    pub(crate) address: String,
    cue_text: String,
    /// Latest cue and when it arrived (egui time).
    banner: Option<(Cue, f64)>,
    log: Vec<String>,
}

impl LiveUi {
    pub(crate) fn following(&self) -> bool {
        matches!(self.mode, Mode::Following { .. })
    }

    /// "3 connected · 2 ready" while leading.
    pub(crate) fn summary(&self) -> Option<String> {
        match &self.mode {
            Mode::Leading { leader, .. } => Some(tr!(
                "Live: {members} in the band, {ready} ready",
                members = leader.members().len(),
                ready = leader.ready_count()
            )),
            _ => None,
        }
    }
}

const QUICK_CUES: [&str; 6] = [
    "Last chorus",
    "Chorus again",
    "Extend solo",
    "Slower",
    "Faster",
    "Stop after this bar",
];

fn log(app: &mut GigCraftApp, line: String) {
    let log = &mut app.live.log;
    log.push(line);
    if log.len() > 100 {
        log.drain(..log.len() - 100);
    }
}

fn now_ms(time: f64) -> u64 {
    (time * 1000.0) as u64
}

/// Every frame: network in, show changes out.
pub(crate) fn pump(app: &mut GigCraftApp, ctx: &egui::Context, time: f64) {
    if app.hooks.live.is_none() || matches!(app.live.mode, Mode::Off) {
        return;
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
    if matches!(app.live.mode, Mode::Leading { .. }) {
        lead_pump(app, time);
    } else {
        follow_pump(app, time);
    }
}

fn carry_out(app: &mut GigCraftApp, outs: Vec<LeaderOut>, time: f64) {
    for out in outs {
        match out {
            LeaderOut::Send { conn, text } => {
                if let Some(io) = app.hooks.live.as_mut() {
                    io.send(conn, text);
                }
            }
            LeaderOut::Close { conn } => {
                if let Some(io) = app.hooks.live.as_mut() {
                    io.close(conn);
                }
            }
            LeaderOut::Event(ev) => match ev {
                LeaderEvent::Joined(m) => log(app, tr!("{name} joined", name = member_label(&m))),
                LeaderEvent::Left(m) => log(app, tr!("{name} left", name = member_label(&m))),
                LeaderEvent::ReadyChanged(_) => {}
                LeaderEvent::Cue(cue) => app.live.banner = Some((cue, time)),
                LeaderEvent::Rejected { conn, reason } => {
                    log(app, format!("#{conn}: {reason}"));
                }
            },
        }
    }
}

fn member_label(m: &gigcraft_live::Member) -> String {
    if m.instrument.is_empty() {
        m.name.clone()
    } else {
        format!("{} ({})", m.name, m.instrument)
    }
}

/// The key of what the leader last published, to send only on change.
fn show_key(app: &GigCraftApp) -> String {
    match &app.perform {
        Some(p) => {
            let st = p.state();
            format!("{}/{}/{}", st["setlist"], st["index"], st["part"])
        }
        None => "off".into(),
    }
}

fn lead_pump(app: &mut GigCraftApp, time: f64) {
    let events = app
        .hooks
        .live
        .as_mut()
        .map(|io| io.poll_host())
        .unwrap_or_default();
    for ev in events {
        let outs = match (&mut app.live.mode, ev) {
            (Mode::Leading { leader, .. }, HostEvent::Text { conn, text }) => {
                leader.on_text(conn as ConnId, &text, now_ms(time))
            }
            (Mode::Leading { leader, .. }, HostEvent::Closed { conn }) => {
                leader.disconnected(conn as ConnId)
            }
            _ => Vec::new(),
        };
        carry_out(app, outs, time);
    }
    let key = show_key(app);
    let changed = matches!(&app.live.mode, Mode::Leading { published, .. } if published.as_deref() != Some(key.as_str()));
    if changed {
        let snapshot: Option<Snapshot> = if app.perform.is_some() {
            app.run("perform.snapshot", Value::Null)
                .and_then(|o| serde_json::from_value(o.result["snapshot"].clone()).ok())
        } else {
            None
        };
        let outs = match &mut app.live.mode {
            Mode::Leading {
                leader, published, ..
            } => {
                *published = Some(key);
                leader.set_snapshot(snapshot)
            }
            _ => Vec::new(),
        };
        carry_out(app, outs, time);
    }
}

fn follow_pump(app: &mut GigCraftApp, time: f64) {
    let events = app
        .hooks
        .live
        .as_mut()
        .map(|io| io.poll_join())
        .unwrap_or_default();
    for ev in events {
        match ev {
            JoinEvent::Connected => {
                let join = match &mut app.live.mode {
                    Mode::Following {
                        follower,
                        connected,
                        status,
                        ..
                    } => {
                        *connected = true;
                        *status = tr!("connected").to_string();
                        Some(follower.join())
                    }
                    _ => None,
                };
                if let (Some(text), Some(io)) = (join, app.hooks.live.as_mut()) {
                    io.send_to_leader(text);
                }
            }
            JoinEvent::Text(text) => {
                let result = match &mut app.live.mode {
                    Mode::Following { follower, .. } => follower.on_text(&text),
                    _ => Ok(Vec::new()),
                };
                match result {
                    Ok(evs) => {
                        for e in evs {
                            follower_event(app, e, time);
                        }
                    }
                    Err(e) => log(app, e),
                }
            }
            JoinEvent::Closed(reason) => {
                if let Mode::Following {
                    connected, status, ..
                } = &mut app.live.mode
                {
                    *connected = false;
                    *status = tr!("disconnected: {reason}", reason = reason);
                }
            }
        }
    }
}

fn follower_event(app: &mut GigCraftApp, e: FollowerEvent, time: f64) {
    match e {
        FollowerEvent::Joined { leader } => log(app, tr!("Following {leader}", leader = leader)),
        FollowerEvent::Moved { part_changed, .. } => {
            app.chart_cache = chart_view::Cache::default();
            if part_changed {
                send_own_presets(app, time);
            }
        }
        FollowerEvent::Roster => {}
        FollowerEvent::Cue(cue) => app.live.banner = Some((cue, time)),
        FollowerEvent::Ended { reason } => {
            log(app, tr!("Session ended: {reason}", reason = reason));
            if let Mode::Following { status, .. } = &mut app.live.mode {
                *status = tr!("Session ended: {reason}", reason = reason);
            }
        }
    }
}

/// The follower's own presets for the leader's current part, from this
/// library, for the devices in this computer's rig only.
fn send_own_presets(app: &mut GigCraftApp, time: f64) {
    let Mode::Following { follower, .. } = &app.live.mode else {
        return;
    };
    let Some(snap) = follower.snapshot() else {
        return;
    };
    let Some(slot) = snap.song() else {
        return;
    };
    let Some(song) = app
        .engine
        .as_ref()
        .and_then(|e| e.library().song(slot.song))
    else {
        return;
    };
    let devices: Vec<String> = app
        .rig
        .config()
        .devices
        .iter()
        .map(|d| d.name.clone())
        .collect();
    let actions: Vec<_> = song
        .parts
        .get(snap.part)
        .map(|p| p.actions.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|a| devices.contains(&a.device))
        .collect();
    if actions.is_empty() {
        return;
    }
    let outs = app.rig.send_part(&actions, crate::rig_ui::now_ms(time));
    crate::rig_ui::handle(app, outs, time);
}

pub(crate) fn start_host(app: &mut GigCraftApp) {
    let Some(io) = app.hooks.live.as_mut() else {
        return;
    };
    match io.host() {
        Ok(h) => {
            let name = if app.live.name.trim().is_empty() {
                tr!("Band leader").to_string()
            } else {
                app.live.name.clone()
            };
            app.live.mode = Mode::Leading {
                leader: Leader::new(h.session, name),
                urls: h.urls,
                published: None,
            };
            log(app, tr!("Live session started").to_string());
        }
        Err(e) => app.message = Some((tr!("Could not start: {error}", error = e), true)),
    }
}

fn stop(app: &mut GigCraftApp, time: f64) {
    match std::mem::take(&mut app.live.mode) {
        Mode::Leading { mut leader, .. } => {
            let outs = leader.stop(tr!("The leader ended the session"));
            app.live.mode = Mode::Leading {
                leader,
                urls: Vec::new(),
                published: None,
            };
            carry_out(app, outs, time);
            app.live.mode = Mode::Off;
            if let Some(io) = app.hooks.live.as_mut() {
                io.stop_host();
            }
        }
        Mode::Following { .. } => {
            if let Some(io) = app.hooks.live.as_mut() {
                io.leave();
            }
        }
        Mode::Off => {}
    }
    app.live.banner = None;
}

pub(crate) fn start_join(app: &mut GigCraftApp) {
    let address = app.live.address.trim().to_string();
    if address.is_empty() {
        return;
    }
    if let Some(io) = app.hooks.live.as_mut() {
        io.join(&address);
    }
    app.live.mode = Mode::Following {
        follower: Follower::new(&app.live.name, &app.live.instrument),
        address,
        connected: false,
        status: tr!("connecting…").to_string(),
    };
    app.live.open = false;
}

/// Send a cue to the band (leader or follower).
fn send_cue(app: &mut GigCraftApp, text: &str, time: f64) {
    match &mut app.live.mode {
        Mode::Leading { leader, .. } => {
            let outs = leader.cue(text, now_ms(time));
            carry_out(app, outs, time);
        }
        Mode::Following { follower, .. } => {
            let msg = follower.cue(text);
            if let Some(io) = app.hooks.live.as_mut() {
                io.send_to_leader(msg);
            }
        }
        Mode::Off => {}
    }
}

fn cue_buttons(app: &mut GigCraftApp, ui: &mut egui::Ui, time: f64) {
    let mut send = None;
    ui.horizontal_wrapped(|ui| {
        for c in QUICK_CUES {
            if ui
                .button(crate::i18n::t(c))
                .on_hover_text(tr!("Show this to the whole band"))
                .clicked()
            {
                send = Some(crate::i18n::t(c).to_string());
            }
        }
    });
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.live.cue_text)
                .hint_text(tr!("Cue to the band"))
                .desired_width(260.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui
            .button(tr!("Send"))
            .on_hover_text(tr!("Send the text to every band member (Enter)"))
            .clicked()
            || enter)
            && !app.live.cue_text.trim().is_empty()
        {
            send = Some(std::mem::take(&mut app.live.cue_text));
        }
    });
    if let Some(text) = send {
        send_cue(app, &text, time);
    }
}

fn qr_image(ui: &mut egui::Ui, modules: &[Vec<bool>], size: f32) {
    let n = modules.len() as f32 + 8.0; // quiet zone of 4 modules
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Image, true, tr!("QR code of the address"))
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::WHITE);
    let cell = size / n;
    for (y, row) in modules.iter().enumerate() {
        for (x, dark) in row.iter().enumerate() {
            if *dark {
                let min = rect.min + egui::vec2((x as f32 + 4.0) * cell, (y as f32 + 4.0) * cell);
                painter.rect_filled(
                    egui::Rect::from_min_size(min, egui::vec2(cell.ceil(), cell.ceil())),
                    0.0,
                    egui::Color32::BLACK,
                );
            }
        }
    }
}

/// The Live window: start a session or join one.
pub(crate) fn window(app: &mut GigCraftApp, ctx: &egui::Context, time: f64) {
    if !app.live.open {
        return;
    }
    let mut open = true;
    let mut action: Option<&str> = None;
    egui::Window::new(tr!("Live: play together"))
        .open(&mut open)
        .default_width(520.0)
        .show(ctx, |ui| {
            if app.hooks.live.is_none() {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    tr!("Live sessions need the desktop app."),
                );
                return;
            }
            egui::Grid::new("live-me").num_columns(2).show(ui, |ui| {
                ui.label(tr!("Your name"));
                ui.text_edit_singleline(&mut app.live.name);
                ui.end_row();
                ui.label(tr!("Instrument"));
                ui.text_edit_singleline(&mut app.live.instrument);
                ui.end_row();
            });
            ui.separator();
            match &app.live.mode {
                Mode::Off => {
                    crate::theme::section(ui, tr!("Lead the band"));
                    ui.label(tr!("Band members follow your setlist on their phones, tablets or computers – on the same Wi-Fi, no app needed."));
                    if ui.add(crate::theme::primary(tr!("Start live session"))).on_hover_text(tr!("Serve the band viewer on this network and show its address and QR code")).clicked() {
                        action = Some("host");
                    }
                    ui.add_space(8.0);
                    crate::theme::section(ui, tr!("Follow a leader"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.live.address)
                                .hint_text("192.168.1.20:8787")
                                .desired_width(220.0),
                        );
                        if ui.button(tr!("Join")).on_hover_text(tr!("Follow the leader at this address")).clicked() {
                            action = Some("join");
                        }
                    });
                    ui.small(tr!("You see the leader's song with your own chart and key, and your own presets are sent for your devices."));
                }
                Mode::Leading { leader, urls, .. } => {
                    let first = urls.first().cloned();
                    ui.horizontal(|ui| {
                        if let Some(url) = &first
                            && let Some(q) = app.hooks.live.as_ref().and_then(|io| io.qr(url))
                        {
                            qr_image(ui, &q, 180.0);
                        }
                        ui.vertical(|ui| {
                            ui.label(tr!("Band members open this address on the same Wi-Fi:"));
                            for url in urls {
                                ui.label(egui::RichText::new(url).monospace().size(18.0));
                            }
                            if urls.is_empty() {
                                ui.colored_label(
                                    ui.visuals().warn_fg_color,
                                    tr!("No network found. Connect to a Wi-Fi or a router."),
                                );
                            }
                            ui.add_space(6.0);
                            ui.label(tr!("Other GigCraft apps: Live…, then Join with this address."));
                        });
                    });
                    ui.separator();
                    let members = leader.members();
                    ui.strong(trn!(
                        members.len(),
                        "{n} band member",
                        "{n} band members"
                    ));
                    for m in &members {
                        ui.horizontal(|ui| {
                            ui.label(member_label(m));
                            let p = crate::theme::pal(ui);
                            if m.ready {
                                crate::theme::led(ui, p.ok, tr!("ready"));
                                ui.colored_label(p.ok, tr!("ready"));
                            } else {
                                crate::theme::led(ui, p.dim, tr!("not ready"));
                            }
                        });
                    }
                    ui.separator();
                    if ui.button(tr!("End live session")).on_hover_text(tr!("Tell everyone the session is over and stop serving")).clicked() {
                        action = Some("stop");
                    }
                }
                Mode::Following {
                    address, status, ..
                } => {
                    ui.label(tr!("Following {address}", address = address));
                    ui.label(status.as_str());
                    if ui.button(tr!("Leave")).on_hover_text(tr!("Stop following the leader")).clicked() {
                        action = Some("stop");
                    }
                }
            }
            if !matches!(app.live.mode, Mode::Off) {
                ui.separator();
                crate::theme::section(ui, tr!("Cues"));
                cue_buttons(app, ui, time);
            }
            egui::CollapsingHeader::new(tr!("Log")).show(ui, |ui| {
                for line in &app.live.log {
                    ui.small(line);
                }
            });
        });
    app.live.open = open;
    match action {
        Some("host") => start_host(app),
        Some("join") => start_join(app),
        Some("stop") => stop(app, time),
        _ => {}
    }
}

/// The latest cue, big, for eight seconds.
pub(crate) fn cue_banner(live: &mut LiveUi, ui: &mut egui::Ui, time: f64, size: f32) {
    let Some((cue, at)) = live.banner.clone() else {
        return;
    };
    if time - at > 8.0 {
        live.banner = None;
        return;
    }
    let fill = egui::Color32::from_rgb(255, 181, 71);
    let r = egui::Frame::new()
        .fill(fill)
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(format!("{}: {}", cue.from, cue.text))
                    .size(size * 1.1)
                    .strong()
                    .color(egui::Color32::BLACK),
            );
        })
        .response
        .interact(egui::Sense::click());
    if r.clicked() {
        live.banner = None;
    }
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(250));
}

/// The follower's stage view.
pub(crate) fn follow_view(app: &mut GigCraftApp, ui: &mut egui::Ui) {
    let time = ui.input(|i| i.time);
    let size = app.font_size * 1.25;
    let (snap, leader, connected, status, ready) = match &app.live.mode {
        Mode::Following {
            follower,
            connected,
            status,
            ..
        } => (
            follower.snapshot().cloned(),
            follower.leader().to_string(),
            *connected,
            status.clone(),
            follower.ready(),
        ),
        _ => return,
    };
    let mut leave = false;
    let mut toggle_ready = false;
    egui::Panel::top("follow-top").show(ui, |ui| {
        ui.horizontal(|ui| {
            let p = crate::theme::pal(ui);
            crate::theme::led(ui, if connected { p.ok } else { p.bad }, &status);
            if let Some(s) = &snap {
                ui.label(egui::RichText::new(&s.setlist_name).strong());
                crate::theme::readout(
                    ui,
                    tr!("Song"),
                    &format!("{}/{}", s.index + 1, s.count),
                    15.0,
                );
            }
            if !leader.is_empty() {
                ui.label(tr!("Leader: {name}", name = leader));
            }
            if !connected {
                ui.label(status.as_str());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(tr!("Leave"))
                    .on_hover_text(tr!("Stop following the leader"))
                    .clicked()
                {
                    leave = true;
                }
            });
        });
    });
    egui::Panel::bottom("follow-bottom").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let pal = crate::theme::palette(true);
            let button = if ready {
                egui::Button::new(egui::RichText::new(tr!("Ready")).size(size))
                    .stroke(egui::Stroke::new(2.0, pal.ok))
            } else {
                egui::Button::new(
                    egui::RichText::new(tr!("Ready"))
                        .size(size)
                        .color(pal.on_accent),
                )
                .fill(pal.accent)
            };
            if ready {
                crate::theme::led(ui, pal.ok, tr!("ready"));
            }
            if ui
                .add(button.min_size(egui::vec2(160.0, size * 2.0)))
                .on_hover_text(tr!("Tell the band you are ready for this song"))
                .clicked()
            {
                toggle_ready = true;
            }
            ui.separator();
            let next = snap.as_ref().and_then(|s| s.next.as_ref());
            let text = match next {
                Some(Slot::Song(n)) => tr!(
                    "Up next: {title}",
                    title = format!(
                        "{}{}",
                        n.title,
                        n.key
                            .as_ref()
                            .map(|k| format!("  [{k}]"))
                            .unwrap_or_default()
                    )
                ),
                Some(Slot::Break { minutes, .. }) => {
                    tr!("Up next: break {minutes} min", minutes = minutes)
                }
                Some(Slot::Missing) => tr!("Up next: (missing song)").into(),
                None => tr!("Last song").into(),
            };
            ui.label(egui::RichText::new(text).size(size * 0.8));
        });
        ui.add_space(4.0);
        cue_buttons(app, ui, time);
        ui.add_space(6.0);
    });
    egui::CentralPanel::default().show(ui, |ui| {
        cue_banner(&mut app.live, ui, time, size);
        let Some(s) = &snap else {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new(tr!("Waiting for the show to start…")).size(size));
            });
            return;
        };
        match &s.current {
            Some(Slot::Song(song)) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&song.title).size(size * 1.6).strong());
                    ui.add_space(8.0);
                    if let Some(k) = &song.key {
                        crate::theme::readout(ui, tr!("Key"), k, size * 0.8);
                    }
                    if let Some(t) = song.tempo_bpm {
                        crate::theme::readout(ui, "BPM", &format!("{t:.0}"), size * 0.8);
                    }
                });
                if song.part_names.len() > 1 {
                    ui.horizontal_wrapped(|ui| {
                        for (i, name) in song.part_names.iter().enumerate() {
                            let _ = ui.selectable_label(
                                i == s.part,
                                egui::RichText::new(name).size(size * 0.8),
                            );
                        }
                    });
                }
                if !song.gig_notes.is_empty() {
                    ui.label(
                        egui::RichText::new(&song.gig_notes)
                            .size(size)
                            .color(ui.visuals().warn_fg_color),
                    );
                }
                ui.separator();
                // Own chart if this library has the song, else the leader's.
                let own = app.engine.as_ref().and_then(|e| {
                    let lib = e.library();
                    let stored = lib.song(song.song)?;
                    let semis = match (stored.key.as_deref(), song.key.as_deref()) {
                        (Some(from), Some(to)) => {
                            gigcraft_chordpro::semitones_between(from, to).unwrap_or(0)
                        }
                        _ => 0,
                    };
                    Some(app.chart_cache.get(lib, song.song, semis))
                });
                let parsed = own.or_else(|| {
                    song.chart_source
                        .as_deref()
                        .map(|src| gigcraft_chordpro::parse(src, song.semitones))
                });
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| match parsed {
                        Some(p) => chart_view::show(ui, &p.chart, size),
                        None => {
                            ui.label(tr!("No chart yet."));
                        }
                    });
            }
            Some(Slot::Break { label, minutes }) => {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    let label = if label.is_empty() {
                        tr!("Break")
                    } else {
                        label
                    };
                    ui.label(egui::RichText::new(label).size(size * 2.0).strong());
                    ui.label(
                        egui::RichText::new(tr!("{minutes} min", minutes = minutes))
                            .size(size * 1.5),
                    );
                });
            }
            Some(Slot::Missing) => {
                ui.label(
                    egui::RichText::new(tr!("This song is no longer in the library.")).size(size),
                );
            }
            None => {
                ui.label(egui::RichText::new(tr!("This setlist is empty.")).size(size));
            }
        }
    });
    if toggle_ready {
        let msg = match &mut app.live.mode {
            Mode::Following { follower, .. } => {
                let now = follower.ready();
                Some(follower.set_ready(!now))
            }
            _ => None,
        };
        if let (Some(msg), Some(io)) = (msg, app.hooks.live.as_mut()) {
            io.send_to_leader(msg);
        }
    }
    if leave {
        stop(app, time);
        app.live.mode = Mode::Off;
    }
}

/// Settings kept between runs.
pub(crate) fn load(app: &mut GigCraftApp, storage: &dyn eframe::Storage) {
    if let Some(v) = storage.get_string(NAME_KEY) {
        app.live.name = v;
    }
    if let Some(v) = storage.get_string(INSTRUMENT_KEY) {
        app.live.instrument = v;
    }
    if let Some(v) = storage.get_string(ADDRESS_KEY) {
        app.live.address = v;
    }
}

pub(crate) fn save(app: &GigCraftApp, storage: &mut dyn eframe::Storage) {
    storage.set_string(NAME_KEY, app.live.name.clone());
    storage.set_string(INSTRUMENT_KEY, app.live.instrument.clone());
    storage.set_string(ADDRESS_KEY, app.live.address.clone());
}

const NAME_KEY: &str = "gigcraft.live_name";
const INSTRUMENT_KEY: &str = "gigcraft.live_instrument";
const ADDRESS_KEY: &str = "gigcraft.live_address";
