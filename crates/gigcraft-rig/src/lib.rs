//! The rig: the musician's MIDI devices as GigCraft sees them.
//!
//! A [`Rig`] knows which device names used in songs ("Play Acoustic",
//! "Helix") belong to which MIDI port and device profile. It turns a song
//! part's [`MidiAction`]s into bytes for the right ports, drives devices that
//! need more than Program Change (the TC-Helicon Play Acoustic driver), and
//! turns incoming MIDI into stage [`Action`]s through learned triggers.
//!
//! Like the rest of the core it does no I/O and reads no clock: the platform
//! (midir on desktop, CoreMIDI on iOS) passes in port names, incoming bytes
//! and the time, and sends out what the rig returns. A device can be
//! *simulated*, so everything can be tried and tested without hardware.

use gigcraft_midi::play_acoustic::{
    self, Driver, Notice as PaNotice, ProgramChangeSupport, SimulatedPlayAcoustic,
};
use gigcraft_midi::profile::{self, DeviceProfile};
use gigcraft_midi::{Event, StreamParser, Switching, Trigger};
use gigcraft_model::MidiAction;
use gigcraft_perform::Action;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Longest a scheduled message may wait (guards against bad gaps).
const MAX_DELAY_MS: u64 = 10_000;

/// One device of the rig.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// The name songs use in their MIDI actions.
    pub name: String,
    /// MIDI port (exact name or part of it); `None` = pick automatically where possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// Device profile id (see `device-profiles/`).
    #[serde(default = "generic")]
    pub profile: String,
    /// No hardware: a simulated device answers (for rehearsing and demos).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub simulated: bool,
    /// Learned by the Play Acoustic driver: "works", "ignored" or absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_change: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pc_offset: Option<i32>,
}

fn generic() -> String {
    profile::GENERIC_MIDI.to_string()
}

/// A learned MIDI trigger for a stage action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mapping {
    pub trigger: Trigger,
    pub action: Action,
    /// Only from this input port (part of its name); `None` = any port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
}

/// The rig as stored on a device (not in the library: ports differ per computer).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RigConfig {
    #[serde(default)]
    pub devices: Vec<DeviceConfig>,
    #[serde(default)]
    pub mappings: Vec<Mapping>,
    /// Pause between messages of one part, in ms.
    #[serde(default)]
    pub gap_ms: u64,
}

impl RigConfig {
    /// A rig with one simulated Play Acoustic, matching the demo library.
    #[must_use]
    pub fn demo() -> Self {
        Self {
            devices: vec![DeviceConfig {
                name: "Play Acoustic".into(),
                port: None,
                profile: profile::PLAY_ACOUSTIC.to_string(),
                simulated: true,
                program_change: None,
                pc_offset: None,
            }],
            mappings: Vec::new(),
            gap_ms: 0,
        }
    }
}

/// How important a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Ok,
    Warning,
    Error,
}

/// What the platform should do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RigOut {
    /// Send one MIDI message to an output port.
    Send { port: String, bytes: Vec<u8> },
    /// Tell the musician (status line).
    Notice {
        device: String,
        level: Level,
        text: String,
    },
    /// A stage action triggered by MIDI input.
    Action { action: Action },
    /// MIDI Learn caught this trigger.
    Learned { port: String, trigger: Trigger },
    /// The configuration changed (something was learned); save it.
    ConfigChanged,
}

/// Status of one device for the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceStatus {
    pub name: String,
    pub profile: String,
    pub port: Option<String>,
    pub simulated: bool,
    pub state: String,
    pub current: Option<String>,
    pub busy: bool,
}

#[derive(Debug)]
struct PaDevice {
    driver: Driver,
    sim: Option<SimulatedPlayAcoustic>,
    /// The output port the driver is connected through (None while simulated).
    port: Option<String>,
}

/// Devices, drivers and MIDI input routing.
#[derive(Debug)]
pub struct Rig {
    config: RigConfig,
    profiles: Vec<DeviceProfile>,
    outputs: Vec<String>,
    pa: BTreeMap<String, PaDevice>,
    parsers: BTreeMap<String, StreamParser>,
    queue: Vec<(u64, String, Vec<u8>)>,
    learning: bool,
}

fn eq(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn port_matches(want: &str, port: &str) -> bool {
    let (w, p) = (want.to_lowercase(), port.to_lowercase());
    !w.is_empty() && (p == w || p.contains(&w))
}

impl Rig {
    #[must_use]
    pub fn new(config: RigConfig) -> Self {
        let mut rig = Self {
            config: RigConfig::default(),
            profiles: profile::builtin(),
            outputs: Vec::new(),
            pa: BTreeMap::new(),
            parsers: BTreeMap::new(),
            queue: Vec::new(),
            learning: false,
        };
        let _ = rig.set_config(config, 0);
        rig
    }

    #[must_use]
    pub fn config(&self) -> &RigConfig {
        &self.config
    }

    /// All device profiles the rig knows.
    #[must_use]
    pub fn profiles(&self) -> &[DeviceProfile] {
        &self.profiles
    }

    fn profile(&self, id: &str) -> Option<&DeviceProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    fn is_stepping(&self, d: &DeviceConfig) -> bool {
        self.profile(&d.profile)
            .is_some_and(|p| p.switching == Switching::PlayAcousticStepping)
    }

    /// Replace the configuration; drivers are rebuilt and (re)connected.
    pub fn set_config(&mut self, config: RigConfig, now: u64) -> Vec<RigOut> {
        self.config = config;
        self.pa.clear();
        self.queue.clear();
        let devices = self.config.devices.clone();
        let mut out = Vec::new();
        let stepping: Vec<DeviceConfig> = devices
            .into_iter()
            .filter(|d| self.is_stepping(d))
            .collect();
        for d in &stepping {
            let cfg = play_acoustic::Config {
                program_change: match d.program_change.as_deref() {
                    Some("works") => ProgramChangeSupport::Works,
                    Some("ignored") => ProgramChangeSupport::Ignored,
                    _ => ProgramChangeSupport::Unknown,
                },
                pc_offset: d.pc_offset,
                max_preset: self
                    .profile(&d.profile)
                    .map(|p| p.preset_count)
                    .filter(|n| *n > 0),
                ..play_acoustic::Config::default()
            };
            let sim = d.simulated.then(|| {
                SimulatedPlayAcoustic::new(1, 1..=112).with_names([
                    (1, "Warm Strum"),
                    (2, "Big Chorus"),
                    (3, "Ambient Pad"),
                    (4, "Bluesy Drive"),
                ])
            });
            self.pa.insert(
                d.name.clone(),
                PaDevice {
                    driver: Driver::new(cfg),
                    sim,
                    port: None,
                },
            );
        }
        out.extend(self.reconnect(now));
        out
    }

    /// The platform's current list of MIDI output ports.
    pub fn ports_changed(&mut self, outputs: &[String], now: u64) -> Vec<RigOut> {
        self.outputs = outputs.to_vec();
        self.reconnect(now)
    }

    /// The output port for a device, if one is available.
    fn resolve_port(&self, d: &DeviceConfig) -> Option<String> {
        if d.simulated {
            return None;
        }
        match &d.port {
            Some(want) => self.outputs.iter().find(|p| port_matches(want, p)).cloned(),
            None if self.is_stepping(d) => self
                .outputs
                .iter()
                .find(|p| {
                    let l = p.to_lowercase();
                    l.contains("play acoustic") || l.contains("play electric")
                })
                .cloned(),
            None => None,
        }
    }

    fn reconnect(&mut self, now: u64) -> Vec<RigOut> {
        let mut out = Vec::new();
        let devices = self.config.devices.clone();
        let devices: Vec<DeviceConfig> = devices
            .into_iter()
            .filter(|d| self.pa.contains_key(&d.name))
            .collect();
        for d in &devices {
            let port = self.resolve_port(d);
            let Some(dev) = self.pa.get_mut(&d.name) else {
                continue;
            };
            if d.simulated {
                if matches!(dev.driver.state(), play_acoustic::State::Idle) {
                    let o = dev.driver.connect(now);
                    out.extend(self.pump(&d.name, o, now));
                }
                continue;
            }
            if dev.port == port {
                continue;
            }
            let mut outputs = dev.driver.disconnect();
            dev.port = port.clone();
            if port.is_some() {
                outputs.extend(dev.driver.connect(now));
            } else {
                out.push(RigOut::Notice {
                    device: d.name.clone(),
                    level: Level::Warning,
                    text: "not connected".into(),
                });
            }
            out.extend(self.pump(&d.name, outputs, now));
        }
        out
    }

    /// Driver outputs → rig outputs. For a simulated device the bytes go to
    /// the simulation and its answers straight back into the driver.
    fn pump(
        &mut self,
        device: &str,
        mut pending: Vec<play_acoustic::Output>,
        now: u64,
    ) -> Vec<RigOut> {
        let mut out = Vec::new();
        let mut rounds = 0;
        while !pending.is_empty() && rounds < 64 {
            rounds += 1;
            let mut next = Vec::new();
            for o in pending {
                match o {
                    play_acoustic::Output::Send(bytes) => {
                        let Some(dev) = self.pa.get_mut(device) else {
                            continue;
                        };
                        if let Some(sim) = dev.sim.as_mut() {
                            for reply in sim.receive(&bytes) {
                                next.extend(dev.driver.on_message(&reply, now));
                            }
                        } else if let Some(port) = dev.port.clone() {
                            out.push(RigOut::Send { port, bytes });
                        }
                    }
                    play_acoustic::Output::Notice(n) => out.extend(self.notice(device, n)),
                }
            }
            pending = next;
        }
        out
    }

    fn notice(&mut self, device: &str, n: PaNotice) -> Vec<RigOut> {
        let (level, text) = match n {
            PaNotice::Connected { model } => (Level::Ok, format!("{model} connected")),
            PaNotice::NotSupported { .. } => (
                Level::Error,
                "does not answer as a Play Acoustic / Play Electric".to_string(),
            ),
            PaNotice::CurrentPreset { number, name } => (
                Level::Info,
                format!(
                    "preset {number}{}",
                    name.map(|n| format!(" {n}")).unwrap_or_default()
                ),
            ),
            PaNotice::Stepping { from, to } => (Level::Info, format!("stepping {from} → {to}")),
            PaNotice::Reached { number } => (Level::Ok, format!("at preset {number}")),
            PaNotice::Failed { reason } => (Level::Error, reason.to_string()),
            PaNotice::Lost => (
                Level::Warning,
                "stopped answering; checking again".to_string(),
            ),
            PaNotice::ProgramChangeTried { works } => {
                let (pc, offset) = self.pa.get(device).map_or((None, None), |d| {
                    let pc = match d.driver.program_change() {
                        ProgramChangeSupport::Works => Some("works".to_string()),
                        ProgramChangeSupport::Ignored => Some("ignored".to_string()),
                        ProgramChangeSupport::Unknown => None,
                    };
                    (pc, d.driver.pc_offset())
                });
                if let Some(cfg) = self.config.devices.iter_mut().find(|d| d.name == device) {
                    cfg.program_change = pc;
                    cfg.pc_offset = offset;
                }
                let text = if works {
                    "Program Change works"
                } else {
                    "Program Change ignored; using the buttons"
                };
                return vec![
                    RigOut::Notice {
                        device: device.to_string(),
                        level: Level::Info,
                        text: text.into(),
                    },
                    RigOut::ConfigChanged,
                ];
            }
        };
        vec![RigOut::Notice {
            device: device.to_string(),
            level,
            text,
        }]
    }

    /// Send the MIDI actions of a song part.
    pub fn send_part(&mut self, actions: &[MidiAction], now: u64) -> Vec<RigOut> {
        let mut out = Vec::new();
        let mut generic = Vec::new();
        for a in actions {
            let Some(d) = self
                .config
                .devices
                .iter()
                .find(|d| eq(&d.name, &a.device))
                .cloned()
            else {
                out.push(RigOut::Notice {
                    device: a.device.clone(),
                    level: Level::Warning,
                    text: "no device with this name in the rig settings".into(),
                });
                continue;
            };
            if self.is_stepping(&d) {
                let gigcraft_model::MidiMessage::ProgramChange { program, .. } = a.message else {
                    out.push(RigOut::Notice {
                        device: d.name.clone(),
                        level: Level::Warning,
                        text: "only preset changes are supported for this device".into(),
                    });
                    continue;
                };
                let target = self
                    .profile(&d.profile)
                    .map_or(u16::from(program) + 1, |p| {
                        profile::display_preset_number(p, program)
                    });
                if let Some(dev) = self.pa.get_mut(&d.name) {
                    let o = dev.driver.select_preset(target, now);
                    out.extend(self.pump(&d.name, o, now));
                }
            } else {
                generic.push((d, a.clone()));
            }
        }
        let actions: Vec<MidiAction> = generic.iter().map(|(_, a)| a.clone()).collect();
        let plan = gigcraft_midi::schedule(
            &actions,
            gigcraft_midi::ScheduleOptions {
                start_ms: now,
                gap_ms: self.config.gap_ms,
            },
        );
        for e in plan.errors {
            out.push(RigOut::Notice {
                device: e.device.clone(),
                level: Level::Error,
                text: e.message,
            });
        }
        for m in plan.messages {
            let Some((d, _)) = generic.get(m.action) else {
                continue;
            };
            let Some(port) = self.resolve_port(d) else {
                if !d.simulated {
                    out.push(RigOut::Notice {
                        device: d.name.clone(),
                        level: Level::Warning,
                        text: "no MIDI port; preset not sent".into(),
                    });
                }
                continue;
            };
            if m.at_ms <= now {
                out.push(RigOut::Send {
                    port,
                    bytes: m.bytes,
                });
            } else {
                self.queue
                    .push((m.at_ms.min(now + MAX_DELAY_MS), port, m.bytes));
            }
        }
        out
    }

    /// Incoming MIDI from an input port.
    pub fn on_input(&mut self, port: &str, bytes: &[u8], now: u64) -> Vec<RigOut> {
        let mut out = Vec::new();
        // The Play Acoustic answers on the port it is connected through.
        let names: Vec<String> = self
            .pa
            .iter()
            .filter(|(_, d)| {
                d.port
                    .as_deref()
                    .is_some_and(|p| port_matches(p, port) || port_matches(port, p))
            })
            .map(|(n, _)| n.clone())
            .collect();
        for name in names {
            if let Some(dev) = self.pa.get_mut(&name) {
                let o = dev.driver.on_message(bytes, now);
                out.extend(self.pump(&name, o, now));
            }
        }
        let events = self
            .parsers
            .entry(port.to_string())
            .or_default()
            .events(bytes);
        for ev in events {
            if ev.is_realtime() || matches!(ev, Event::SysEx(_)) {
                continue;
            }
            if self.learning {
                if let Some(trigger) = Trigger::from_event(&ev) {
                    self.learning = false;
                    out.push(RigOut::Learned {
                        port: port.to_string(),
                        trigger,
                    });
                }
                continue;
            }
            for m in &self.config.mappings {
                if m.port.as_deref().is_none_or(|p| port_matches(p, port)) && m.trigger.matches(&ev)
                {
                    out.push(RigOut::Action { action: m.action });
                }
            }
        }
        out
    }

    /// Timeouts and delayed messages. Call at least every 20 ms while
    /// [`Rig::next_deadline`] is `Some`.
    pub fn tick(&mut self, now: u64) -> Vec<RigOut> {
        let mut out = Vec::new();
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.queue)
            .into_iter()
            .partition(|(t, _, _)| *t <= now);
        self.queue = later;
        for (_, port, bytes) in due {
            out.push(RigOut::Send { port, bytes });
        }
        let names: Vec<String> = self.pa.keys().cloned().collect();
        for name in names {
            if let Some(dev) = self.pa.get_mut(&name) {
                let o = dev.driver.tick(now);
                out.extend(self.pump(&name, o, now));
            }
        }
        out
    }

    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        let q = self.queue.iter().map(|(t, _, _)| *t).min();
        let d = self
            .pa
            .values()
            .filter_map(|d| d.driver.next_deadline())
            .min();
        match (q, d) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// The next incoming message becomes a [`RigOut::Learned`] trigger.
    pub fn learn(&mut self, on: bool) {
        self.learning = on;
    }

    #[must_use]
    pub fn is_learning(&self) -> bool {
        self.learning
    }

    /// Map a learned trigger to an action (replacing mappings of the same trigger).
    pub fn add_mapping(&mut self, mapping: Mapping) -> Vec<RigOut> {
        self.config
            .mappings
            .retain(|m| m.trigger != mapping.trigger);
        self.config.mappings.push(mapping);
        vec![RigOut::ConfigChanged]
    }

    /// All notes off and all sound off on every channel of every generic port;
    /// pending messages and preset walks are cancelled.
    pub fn panic(&mut self, now: u64) -> Vec<RigOut> {
        self.queue.clear();
        let mut out = Vec::new();
        let devices = self.config.devices.clone();
        let mut ports: Vec<String> = devices
            .iter()
            .filter(|d| !self.is_stepping(d))
            .filter_map(|d| self.resolve_port(d))
            .collect();
        ports.sort();
        ports.dedup();
        for port in ports {
            for ch in 0u8..16 {
                out.push(RigOut::Send {
                    port: port.clone(),
                    bytes: vec![0xB0 | ch, 120, 0],
                });
                out.push(RigOut::Send {
                    port: port.clone(),
                    bytes: vec![0xB0 | ch, 123, 0],
                });
            }
        }
        // Stop preset walks by aiming at where the pedal is now.
        let names: Vec<String> = self.pa.keys().cloned().collect();
        for name in names {
            if let Some(dev) = self.pa.get_mut(&name)
                && dev.driver.is_busy()
                && let Some(cur) = dev.driver.current().map(|p| p.number)
            {
                let o = dev.driver.select_preset(cur, now);
                out.extend(self.pump(&name, o, now));
            }
        }
        out
    }

    /// Status of every device.
    #[must_use]
    pub fn status(&self) -> Vec<DeviceStatus> {
        self.config
            .devices
            .iter()
            .map(|d| {
                let pa = self.pa.get(&d.name);
                let state = match pa.map(|p| p.driver.state()) {
                    Some(play_acoustic::State::Idle) => "not connected".to_string(),
                    Some(play_acoustic::State::Identifying) => "connecting".to_string(),
                    Some(play_acoustic::State::Ready { model }) => format!("{model} ready"),
                    Some(play_acoustic::State::NotSupported) => "not a Play Acoustic".to_string(),
                    Some(play_acoustic::State::Lost { .. }) => "lost".to_string(),
                    None if d.simulated => "simulated".to_string(),
                    None => match self.resolve_port(d) {
                        Some(_) => "ready".to_string(),
                        None => "no port".to_string(),
                    },
                };
                DeviceStatus {
                    name: d.name.clone(),
                    profile: d.profile.clone(),
                    port: pa
                        .and_then(|p| p.port.clone())
                        .or_else(|| self.resolve_port(d)),
                    simulated: d.simulated,
                    state,
                    current: pa.and_then(|p| {
                        p.driver.current().map(|c| match &c.name {
                            Some(n) => format!("{} {n}", c.number),
                            None => c.number.to_string(),
                        })
                    }),
                    busy: pa.is_some_and(|p| p.driver.is_busy()),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
