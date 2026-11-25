//! Driver for the TC-Helicon Play Acoustic and Play Electric, without I/O.
//!
//! A port of Cuebook's pedal logic (PedalLink) to a state machine: the
//! platform feeds it what arrives from the pedal ([`Driver::on_message`]),
//! the time ([`Driver::tick`], at [`Driver::next_deadline`]) and the user's
//! requests ([`Driver::select_preset`], [`Driver::read_current`]); the
//! driver answers with bytes to send and [`Notice`]s for the UI. Time is
//! always passed in, so everything is deterministic.
//!
//! What it does, with the protocol facts documented by the open-acoustic
//! project (`docs/SYSEX.md`, firmware 1.4.00):
//!
//! - Identity request first. Only a device that answers as TC-Helicon
//!   (`00 01 38`) family `70` (Play Acoustic) or `71` (Play Electric) is driven.
//! - Reads the current preset: parameter `0x2A4` with `47 05 24` (reply
//!   `22 05 24 <28-bit value>`) and its name with `45 00 00` (reply: preset
//!   header `20`, name in bytes 11–25).
//! - Switches the front-panel event tunnel on (`55 01 01 00 00 00`) so that
//!   injected button events (`55 02 <28-bit event>`) act.
//! - Selects a preset with Program Change if the pedal honours it (tried
//!   once, the result remembered: the firmware drops Program Change on USB),
//!   otherwise by pressing ▶ / ◀ (`DB 04` / `DB 05`), waiting for the
//!   Program Change the pedal sends after each change and reading `0x2A4`
//!   after every step.
//!
//! **It never writes to the device.** Everything it can send is one of a
//! closed set of requests (identity, get parameter, request current preset,
//! tunnel on, button event, Program Change); there is no "set parameter"
//! (`22`), setup, preset write or delete. [`is_device_write`] tells such
//! messages apart, and the tests check every byte the driver sends.

use crate::identity::{IDENTITY_REQUEST, IdentityReply};
use crate::parser::StreamParser;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::RangeInclusive;

/// TC-Helicon's manufacturer ID.
pub const MANUFACTURER: [u8; 3] = [0x00, 0x01, 0x38];

/// Parameter holding the number of the current preset (read only).
pub const PRESET_PARAM: u16 = 0x2A4;

/// SysEx commands after the `F0 00 01 38 <ch> <product>` header.
pub mod command {
    /// Set parameter (a write; also the device's reply to [`GET_PARAM`]).
    pub const SET_PARAM: u8 = 0x22;
    /// Get parameter `p1 p0`.
    pub const GET_PARAM: u8 = 0x47;
    /// Request preset `hi lo` (`00 00` = the current edit buffer).
    pub const REQUEST_PRESET: u8 = 0x45;
    /// Preset header (with the name); sent to the device it writes a preset.
    pub const PRESET_HEADER: u8 = 0x20;
    /// Preset data packet; sent to the device it writes a preset.
    pub const PRESET_DATA: u8 = 0x21;
    /// Setup packet; sent to the device it writes the global setup.
    pub const SETUP: u8 = 0x13;
    /// Delete preset.
    pub const DELETE_PRESET: u8 = 0x50;
    /// Sync session around setup and preset transfers.
    pub const SYNC_SESSION: u8 = 0x57;
    /// Front-panel event tunnel.
    pub const TUNNEL: u8 = 0x55;
    /// The only commands the driver ever sends.
    pub const SENT_BY_DRIVER: [u8; 3] = [GET_PARAM, REQUEST_PRESET, TUNNEL];
}

/// Front-panel button event code (`DB <button> <00 press | 01 release>`).
const BUTTON_EVENT: u32 = 0xDB;

/// The pedal models the driver accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Model {
    PlayAcoustic,
    PlayElectric,
}

impl Model {
    /// Name as shown to musicians.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PlayAcoustic => "Play Acoustic",
            Self::PlayElectric => "Play Electric",
        }
    }

    /// Product byte in the SysEx header (and identity family code).
    #[must_use]
    pub fn product_code(self) -> u8 {
        match self {
            Self::PlayAcoustic => 0x70,
            Self::PlayElectric => 0x71,
        }
    }

    /// The model an identity reply belongs to, if it is one of these pedals.
    #[must_use]
    pub fn from_identity(reply: &IdentityReply) -> Option<Self> {
        if reply.manufacturer != MANUFACTURER {
            return None;
        }
        match reply.family {
            0x70 => Some(Self::PlayAcoustic),
            0x71 => Some(Self::PlayElectric),
            _ => None,
        }
    }
}

impl fmt::Display for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Whether Program Change selects a preset on this pedal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ProgramChangeSupport {
    /// Not tried yet.
    #[default]
    Unknown,
    /// Tried, and the pedal went to the preset.
    Works,
    /// Tried, and the pedal ignored it (the firmware does this on USB).
    Ignored,
}

/// Timing and limits of the driver. [`Config::default`] suits USB;
/// [`Config::bluetooth`] allows for slower links.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// How long to wait for an identity reply.
    pub identity_timeout_ms: u64,
    /// Identity requests sent before giving up.
    pub identity_attempts: u8,
    /// How long to wait for a parameter reply.
    pub read_timeout_ms: u64,
    /// How long to wait for the preset name.
    pub name_timeout_ms: u64,
    /// How long to wait, after a button release, for the pedal's Program Change.
    pub step_timeout_ms: u64,
    /// How long a button is held down.
    pub press_ms: u64,
    /// How long to wait after sending a Program Change before checking it.
    pub program_change_settle_ms: u64,
    /// Delay before reading the preset after the pedal changed it by itself.
    pub refresh_delay_ms: u64,
    /// Interval between identity checks while the pedal is lost.
    pub retry_interval_ms: u64,
    /// Unanswered reads in a row after which the pedal counts as lost.
    pub max_missed_reads: u8,
    /// Steps in a row that do not move towards the target before giving up.
    pub max_failed_steps: u8,
    /// Steps per selection before giving up.
    pub max_steps: u16,
    /// Highest preset number accepted by [`Driver::select_preset`] (`None` = no limit).
    pub max_preset: Option<u16>,
    /// What is known about Program Change (persist [`Driver::program_change`]).
    pub program_change: ProgramChangeSupport,
    /// Preset number minus program number, learned from the pedal (persist [`Driver::pc_offset`]).
    pub pc_offset: Option<i32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            identity_timeout_ms: 1200,
            identity_attempts: 3,
            read_timeout_ms: 600,
            name_timeout_ms: 1000,
            step_timeout_ms: 900,
            press_ms: 60,
            program_change_settle_ms: 350,
            refresh_delay_ms: 250,
            retry_interval_ms: 4000,
            max_missed_reads: 3,
            max_failed_steps: 3,
            max_steps: 400,
            max_preset: None,
            program_change: ProgramChangeSupport::Unknown,
            pc_offset: None,
        }
    }
}

impl Config {
    /// Timeouts for a Bluetooth MIDI link (slower, and bursts choke it).
    #[must_use]
    pub fn bluetooth() -> Self {
        Self {
            identity_timeout_ms: 2200,
            read_timeout_ms: 1200,
            name_timeout_ms: 2000,
            step_timeout_ms: 1800,
            ..Self::default()
        }
    }
}

/// A preset on the pedal, numbered as the pedal shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub number: u16,
    /// `None` if the pedal did not send the name in time.
    pub name: Option<String>,
}

/// Why a request could not be carried out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailReason {
    /// No pedal is connected and identified.
    NotConnected,
    /// The pedal did not answer a read.
    NoAnswer,
    /// The pedal answered with a value that is not a preset number.
    UnexpectedValue(i32),
    /// The target is not a preset number.
    OutOfRange { target: u16 },
    /// Several steps in a row did not move the pedal towards the target:
    /// it is probably not on its preset screen, or at its first/last preset.
    NoProgress { at: u16, target: u16 },
    /// The step limit was reached.
    TooManySteps { at: u16, target: u16 },
    /// The pedal stopped answering during the request.
    Lost,
}

impl fmt::Display for FailReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("no Play Acoustic is connected"),
            Self::NoAnswer => f.write_str("the pedal did not answer"),
            Self::UnexpectedValue(v) => write!(f, "the pedal reported preset {v}"),
            Self::OutOfRange { target } => write!(f, "there is no preset {target}"),
            Self::NoProgress { at, target } => write!(
                f,
                "stuck at preset {at} on the way to {target}: is the pedal on its preset screen?"
            ),
            Self::TooManySteps { at, target } => {
                write!(f, "gave up at preset {at} on the way to {target}")
            }
            Self::Lost => f.write_str("the pedal stopped answering"),
        }
    }
}

/// What happened, for the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// A Play Acoustic / Play Electric answered and is ready.
    Connected { model: Model },
    /// The device did not answer (`None`) or is something else.
    NotSupported { identity: Option<IdentityReply> },
    /// The pedal's current preset.
    CurrentPreset { number: u16, name: Option<String> },
    /// About to press a button: at `from`, heading for `to`.
    Stepping { from: u16, to: u16 },
    /// The requested preset is selected.
    Reached { number: u16 },
    /// A request failed.
    Failed { reason: FailReason },
    /// Program Change was tried; persist [`Driver::program_change`] and [`Driver::pc_offset`].
    ProgramChangeTried { works: bool },
    /// The pedal stopped answering; the driver checks for it every
    /// [`Config::retry_interval_ms`] and sends `Connected` when it is back.
    Lost,
}

/// One thing the platform must do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Send these bytes (one complete MIDI message) to the pedal.
    Send(Vec<u8>),
    /// Tell the UI.
    Notice(Notice),
}

/// Where the driver is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Not connected ([`Driver::connect`] not called, or [`Driver::disconnect`]ed).
    Idle,
    /// Waiting for the identity reply.
    Identifying,
    /// Identified and ready.
    Ready { model: Model },
    /// The device is not a Play Acoustic / Play Electric, or did not answer.
    NotSupported,
    /// The pedal stopped answering; checking for it again.
    Lost { model: Model },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    Next,
    Previous,
}

impl Button {
    fn code(self) -> u32 {
        match self {
            Self::Next => 0x04,
            Self::Previous => 0x05,
        }
    }
}

/// Everything the driver can send. There is deliberately no write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Request {
    Identity,
    GetParam(u16),
    CurrentPresetName,
    TunnelOn,
    Button { button: Button, press: bool },
    ProgramChange { channel: u8, program: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
    Identity,
    Param,
    Name,
    /// The Program Change the pedal sends after a step.
    Echo,
    /// Only a timer.
    Timer,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    expect: Expect,
    deadline: u64,
}

#[derive(Debug)]
enum Reply {
    Identity(IdentityReply),
    Param(i32),
    Name(String),
    Echo { channel: u8, program: u8 },
    Timeout,
}

#[derive(Debug, Clone, Copy)]
enum ReadPhase {
    Number,
    Name { number: u16 },
}

#[derive(Debug, Clone, Copy)]
enum SelectPhase {
    /// Reading where the pedal is.
    Start,
    /// Program Change sent for `sent_for`; waiting before checking.
    PcSettle {
        sent_for: u16,
    },
    PcVerify {
        sent_for: u16,
    },
    /// Button pressed; waiting for the pedal's Program Change.
    StepEcho {
        from: u16,
        button: Button,
    },
    /// Reading where the step went.
    StepVerify {
        from: u16,
        button: Button,
        echo: Option<u8>,
    },
    /// Arrived at `number`; reading its name.
    Name {
        number: u16,
    },
}

#[derive(Debug, Clone, Copy)]
struct Select {
    target: u16,
    at: Option<u16>,
    phase: SelectPhase,
    steps: u16,
    failed: u8,
    tried_pc: bool,
}

#[derive(Debug, Clone, Copy)]
enum Job {
    Idle,
    Identify { attempt: u8 },
    Probe,
    Read(ReadPhase),
    Select(Select),
}

/// The Play Acoustic / Play Electric driver. See the [module docs](self).
///
/// Each method returns what to do now, in order. Call [`tick`](Self::tick)
/// at [`next_deadline`](Self::next_deadline) (or more often).
#[derive(Debug)]
pub struct Driver {
    config: Config,
    parser: StreamParser,
    state: State,
    model: Model,
    channel: u8,
    tunnel_on: bool,
    current: Option<Preset>,
    pc_channel: u8,
    pending: Option<Pending>,
    job: Job,
    release: Option<(u64, Button)>,
    queued_target: Option<u16>,
    queued_read: bool,
    refresh_at: Option<u64>,
    probe_at: Option<u64>,
    misses: u8,
    out: Vec<Output>,
}

impl Driver {
    /// A driver that is not connected yet.
    #[must_use]
    pub fn new(config: Config) -> Self {
        Self {
            config,
            // Preset transfers are about 0.5 KB; nothing the driver reads is larger.
            parser: StreamParser::with_max_sysex_len(4096),
            state: State::Idle,
            model: Model::PlayAcoustic,
            channel: 0x7F,
            tunnel_on: false,
            current: None,
            pc_channel: 0,
            pending: None,
            job: Job::Idle,
            release: None,
            queued_target: None,
            queued_read: false,
            refresh_at: None,
            probe_at: None,
            misses: 0,
            out: Vec::new(),
        }
    }

    /// Starts over with a newly opened port: sends the identity request.
    pub fn connect(&mut self, now: u64) -> Vec<Output> {
        self.abandon(FailReason::NotConnected);
        self.reset_device();
        self.state = State::Identifying;
        self.job = Job::Identify { attempt: 1 };
        self.send(Request::Identity);
        self.expect(Expect::Identity, now + self.config.identity_timeout_ms);
        self.take()
    }

    /// The port went away.
    pub fn disconnect(&mut self) -> Vec<Output> {
        self.abandon(FailReason::NotConnected);
        self.reset_device();
        self.take()
    }

    /// Bytes from the pedal: complete messages or any part of the stream.
    pub fn on_message(&mut self, bytes: &[u8], now: u64) -> Vec<Output> {
        self.run_timers(now);
        let mut messages = Vec::new();
        self.parser.feed(bytes, |m| messages.push(m.to_vec()));
        for m in &messages {
            self.handle(m, now);
        }
        self.take()
    }

    /// Lets time pass: timeouts, button releases, retries.
    pub fn tick(&mut self, now: u64) -> Vec<Output> {
        self.run_timers(now);
        self.take()
    }

    /// Goes to preset `target` (numbered as the pedal shows it). While a
    /// selection is under way this changes where it is heading.
    pub fn select_preset(&mut self, target: u16, now: u64) -> Vec<Output> {
        self.run_timers(now);
        if !matches!(self.state, State::Ready { .. }) {
            self.notice(Notice::Failed {
                reason: FailReason::NotConnected,
            });
        } else if target == 0 || self.config.max_preset.is_some_and(|max| target > max) {
            self.notice(Notice::Failed {
                reason: FailReason::OutOfRange { target },
            });
        } else {
            match &mut self.job {
                Job::Select(sel) => {
                    if sel.target != target {
                        sel.target = target;
                        if let Some(from) = sel.at {
                            self.notice(Notice::Stepping { from, to: target });
                        }
                    }
                }
                Job::Idle => self.start_select(target, now),
                _ => self.queued_target = Some(target),
            }
        }
        self.take()
    }

    /// Reads the current preset's number and name ([`Notice::CurrentPreset`]).
    pub fn read_current(&mut self, now: u64) -> Vec<Output> {
        self.run_timers(now);
        if !matches!(self.state, State::Ready { .. }) {
            self.notice(Notice::Failed {
                reason: FailReason::NotConnected,
            });
        } else {
            match self.job {
                Job::Idle => self.start_read(now),
                // A selection ends by reading the preset anyway.
                Job::Select(_) => {}
                _ => self.queued_read = true,
            }
        }
        self.take()
    }

    /// When [`tick`](Self::tick) has something to do next, if anything.
    #[must_use]
    pub fn next_deadline(&self) -> Option<u64> {
        let refresh = match (self.state, self.job) {
            (State::Ready { .. }, Job::Idle) => self.refresh_at,
            _ => None,
        };
        [
            self.pending.map(|p| p.deadline),
            self.release.map(|(at, _)| at),
            refresh,
            self.probe_at,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    /// The last preset read from the pedal.
    #[must_use]
    pub fn current(&self) -> Option<&Preset> {
        self.current.as_ref()
    }

    /// Whether a selection or read is under way.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        !matches!(self.job, Job::Idle)
    }

    /// Where a selection under way is heading.
    #[must_use]
    pub fn target(&self) -> Option<u16> {
        match self.job {
            Job::Select(sel) => Some(sel.target),
            _ => self.queued_target,
        }
    }

    /// What is known about Program Change (to keep between sessions).
    #[must_use]
    pub fn program_change(&self) -> ProgramChangeSupport {
        self.config.program_change
    }

    /// Preset number minus program number, if learned (to keep between sessions).
    #[must_use]
    pub fn pc_offset(&self) -> Option<i32> {
        self.config.pc_offset
    }

    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    // ---- plumbing ----

    fn take(&mut self) -> Vec<Output> {
        std::mem::take(&mut self.out)
    }

    fn notice(&mut self, n: Notice) {
        self.out.push(Output::Notice(n));
    }

    fn header(&self) -> [u8; 6] {
        [
            0xF0,
            MANUFACTURER[0],
            MANUFACTURER[1],
            MANUFACTURER[2],
            self.channel,
            self.model.product_code(),
        ]
    }

    fn send(&mut self, request: Request) {
        let mut bytes = Vec::with_capacity(14);
        match request {
            Request::Identity => bytes.extend(IDENTITY_REQUEST),
            Request::GetParam(param) => {
                bytes.extend(self.header());
                bytes.extend([command::GET_PARAM, hi7(param), lo7(param), 0xF7]);
            }
            Request::CurrentPresetName => {
                bytes.extend(self.header());
                bytes.extend([command::REQUEST_PRESET, 0x00, 0x00, 0xF7]);
            }
            Request::TunnelOn => {
                bytes.extend(self.header());
                bytes.extend([command::TUNNEL, 0x01, 0x01, 0x00, 0x00, 0x00, 0xF7]);
            }
            Request::Button { button, press } => {
                // Injected events only act while the tunnel is on.
                if !self.tunnel_on {
                    return;
                }
                let event = BUTTON_EVENT << 16 | button.code() << 8 | u32::from(!press);
                bytes.extend(self.header());
                bytes.extend([command::TUNNEL, 0x02]);
                bytes.extend(encode_28(event as i32));
                bytes.push(0xF7);
            }
            Request::ProgramChange { channel, program } => {
                bytes.extend([0xC0 | (channel & 0x0F), program & 0x7F]);
            }
        }
        self.out.push(Output::Send(bytes));
    }

    fn expect(&mut self, expect: Expect, deadline: u64) {
        self.pending = Some(Pending { expect, deadline });
    }

    fn request_param(&mut self, now: u64) {
        self.send(Request::GetParam(PRESET_PARAM));
        self.expect(Expect::Param, now + self.config.read_timeout_ms);
    }

    fn request_name(&mut self, now: u64) {
        self.send(Request::CurrentPresetName);
        self.expect(Expect::Name, now + self.config.name_timeout_ms);
    }

    fn send_release(&mut self) {
        if let Some((_, button)) = self.release.take() {
            self.send(Request::Button {
                button,
                press: false,
            });
        }
    }

    /// Forgets the device (not the Program Change knowledge in the config).
    fn reset_device(&mut self) {
        self.parser.reset();
        self.state = State::Idle;
        self.model = Model::PlayAcoustic;
        self.channel = 0x7F;
        self.tunnel_on = false;
        self.current = None;
        self.pending = None;
        self.job = Job::Idle;
        self.release = None;
        self.queued_target = None;
        self.queued_read = false;
        self.refresh_at = None;
        self.probe_at = None;
        self.misses = 0;
    }

    /// Ends a selection under way (or queued) with `reason`.
    fn abandon(&mut self, reason: FailReason) {
        let selecting = matches!(self.job, Job::Select(_)) || self.queued_target.is_some();
        self.queued_target = None;
        if matches!(self.job, Job::Select(_)) {
            self.send_release();
            self.job = Job::Idle;
            self.pending = None;
        }
        if selecting {
            self.notice(Notice::Failed { reason });
        }
    }

    fn run_timers(&mut self, now: u64) {
        if self.release.is_some_and(|(at, _)| at <= now) {
            self.send_release();
        }
        if let Some(p) = self.pending
            && p.deadline <= now
        {
            self.pending = None;
            self.advance(Reply::Timeout, now);
        }
        match self.state {
            State::Ready { .. } if matches!(self.job, Job::Idle) => {
                if self.refresh_at.is_some_and(|at| at <= now) {
                    self.refresh_at = None;
                    self.start_read(now);
                } else {
                    self.start_queued(now);
                }
            }
            State::Lost { .. }
                if matches!(self.job, Job::Idle) && self.probe_at.is_some_and(|at| at <= now) =>
            {
                self.probe_at = None;
                self.probe(now);
            }
            _ => {}
        }
    }

    fn handle(&mut self, m: &[u8], now: u64) {
        let reply = match self.pending.map(|p| p.expect) {
            Some(Expect::Identity) => IdentityReply::parse(m).map(Reply::Identity),
            Some(Expect::Param) => param_reply(m, PRESET_PARAM).map(Reply::Param),
            Some(Expect::Name) => current_preset_name(m).map(Reply::Name),
            Some(Expect::Echo) => {
                program_change(m).map(|(channel, program)| Reply::Echo { channel, program })
            }
            Some(Expect::Timer) | None => None,
        };
        match reply {
            Some(reply) => {
                self.pending = None;
                self.advance(reply, now);
            }
            None => self.unsolicited(m, now),
        }
    }

    /// A message nobody waited for: a preset change made on the pedal itself.
    fn unsolicited(&mut self, m: &[u8], now: u64) {
        if let Some((channel, _)) = program_change(m) {
            self.pc_channel = channel;
            if matches!(self.state, State::Ready { .. }) && !matches!(self.job, Job::Select(_)) {
                self.refresh_at = Some(now + self.config.refresh_delay_ms);
            }
        }
    }

    fn advance(&mut self, reply: Reply, now: u64) {
        let job = std::mem::replace(&mut self.job, Job::Idle);
        match job {
            Job::Idle => {}
            Job::Identify { attempt } => self.identify_step(attempt, reply, now),
            Job::Probe => self.probe_step(reply, now),
            Job::Read(phase) => self.read_step(phase, reply, now),
            Job::Select(sel) => self.select_step(sel, reply, now),
        }
        if matches!(self.job, Job::Idle) && matches!(self.state, State::Ready { .. }) {
            self.start_queued(now);
        }
    }

    fn start_queued(&mut self, now: u64) {
        if let Some(target) = self.queued_target.take() {
            self.start_select(target, now);
        } else if std::mem::take(&mut self.queued_read) {
            self.start_read(now);
        }
    }

    // ---- connecting ----

    fn identify_step(&mut self, attempt: u8, reply: Reply, now: u64) {
        match reply {
            Reply::Identity(identity) => match Model::from_identity(&identity) {
                Some(model) => self.became_ready(model, identity.device_id, now),
                None => self.not_supported(Some(identity)),
            },
            _ if attempt < self.config.identity_attempts => {
                self.job = Job::Identify {
                    attempt: attempt + 1,
                };
                self.send(Request::Identity);
                self.expect(Expect::Identity, now + self.config.identity_timeout_ms);
            }
            _ => self.not_supported(None),
        }
    }

    fn not_supported(&mut self, identity: Option<IdentityReply>) {
        self.reset_device();
        self.state = State::NotSupported;
        self.notice(Notice::NotSupported { identity });
    }

    fn became_ready(&mut self, model: Model, channel: u8, now: u64) {
        self.model = model;
        self.channel = channel;
        // The pedal echoes its front panel from now on and takes injected events.
        self.send(Request::TunnelOn);
        self.tunnel_on = true;
        self.misses = 0;
        self.probe_at = None;
        self.state = State::Ready { model };
        self.notice(Notice::Connected { model });
        if let Some(target) = self.queued_target.take() {
            self.start_select(target, now);
        } else {
            self.queued_read = false;
            self.start_read(now);
        }
    }

    /// A read went unanswered. Returns true if the pedal now counts as lost.
    fn missed(&mut self, now: u64) -> bool {
        self.misses = self.misses.saturating_add(1);
        if self.misses < self.config.max_missed_reads {
            return false;
        }
        let model = self.model;
        // Restarted pedals forget the tunnel.
        self.tunnel_on = false;
        self.release = None;
        self.refresh_at = None;
        self.queued_read = false;
        self.state = State::Lost { model };
        self.notice(Notice::Lost);
        self.probe(now);
        true
    }

    fn probe(&mut self, now: u64) {
        self.job = Job::Probe;
        self.send(Request::Identity);
        self.expect(Expect::Identity, now + self.config.identity_timeout_ms);
    }

    fn probe_step(&mut self, reply: Reply, now: u64) {
        match reply {
            Reply::Identity(identity) => match Model::from_identity(&identity) {
                Some(model) => self.became_ready(model, identity.device_id, now),
                None => self.not_supported(Some(identity)),
            },
            _ => self.probe_at = Some(now + self.config.retry_interval_ms),
        }
    }

    // ---- reading ----

    fn start_read(&mut self, now: u64) {
        self.job = Job::Read(ReadPhase::Number);
        self.request_param(now);
    }

    fn read_step(&mut self, phase: ReadPhase, reply: Reply, now: u64) {
        match (phase, reply) {
            (ReadPhase::Number, Reply::Param(v)) => {
                self.misses = 0;
                match u16::try_from(v) {
                    Ok(number) => {
                        self.job = Job::Read(ReadPhase::Name { number });
                        self.request_name(now);
                    }
                    Err(_) => self.notice(Notice::Failed {
                        reason: FailReason::UnexpectedValue(v),
                    }),
                }
            }
            (ReadPhase::Number, _) => {
                if !self.missed(now) {
                    self.notice(Notice::Failed {
                        reason: FailReason::NoAnswer,
                    });
                }
            }
            (ReadPhase::Name { number }, reply) => {
                let name = match reply {
                    Reply::Name(name) => Some(name),
                    _ => None,
                };
                self.set_current(number, name);
            }
        }
    }

    fn set_current(&mut self, number: u16, name: Option<String>) {
        self.current = Some(Preset {
            number,
            name: name.clone(),
        });
        self.notice(Notice::CurrentPreset { number, name });
    }

    // ---- selecting ----

    fn start_select(&mut self, target: u16, now: u64) {
        self.job = Job::Select(Select {
            target,
            at: None,
            phase: SelectPhase::Start,
            steps: 0,
            failed: 0,
            tried_pc: false,
        });
        self.request_param(now);
    }

    fn fail_select(&mut self, reason: FailReason, now: u64) {
        self.send_release();
        self.notice(Notice::Failed { reason });
        // Show where the pedal ended up.
        if matches!(self.state, State::Ready { .. }) {
            self.queued_target = None;
            self.start_read(now);
        }
    }

    /// A read during a selection went unanswered.
    fn select_read_missed(&mut self, now: u64) {
        if self.missed(now) {
            self.send_release();
            self.notice(Notice::Failed {
                reason: FailReason::Lost,
            });
        } else {
            self.send_release();
            self.notice(Notice::Failed {
                reason: FailReason::NoAnswer,
            });
        }
    }

    fn select_step(&mut self, mut sel: Select, reply: Reply, now: u64) {
        match (sel.phase, reply) {
            (SelectPhase::Start, Reply::Param(v)) => {
                self.misses = 0;
                match u16::try_from(v) {
                    Ok(cur) => self.decide(sel, cur, now),
                    Err(_) => self.fail_select(FailReason::UnexpectedValue(v), now),
                }
            }
            (SelectPhase::PcSettle { sent_for }, _) => {
                sel.phase = SelectPhase::PcVerify { sent_for };
                self.job = Job::Select(sel);
                self.request_param(now);
            }
            (SelectPhase::PcVerify { sent_for }, Reply::Param(v)) => {
                self.misses = 0;
                let works = i32::from(sent_for) == v;
                let known = if works {
                    ProgramChangeSupport::Works
                } else {
                    ProgramChangeSupport::Ignored
                };
                if self.config.program_change != known {
                    self.config.program_change = known;
                    self.notice(Notice::ProgramChangeTried { works });
                }
                match u16::try_from(v) {
                    Ok(cur) => self.decide(sel, cur, now),
                    Err(_) => self.fail_select(FailReason::UnexpectedValue(v), now),
                }
            }
            (SelectPhase::StepEcho { from, button }, reply) => {
                self.send_release();
                let echo = match reply {
                    Reply::Echo { channel, program } => {
                        self.pc_channel = channel;
                        Some(program)
                    }
                    _ => None,
                };
                sel.phase = SelectPhase::StepVerify { from, button, echo };
                self.job = Job::Select(sel);
                self.request_param(now);
            }
            (SelectPhase::StepVerify { from, button, echo }, Reply::Param(v)) => {
                self.misses = 0;
                let Ok(cur) = u16::try_from(v) else {
                    self.fail_select(FailReason::UnexpectedValue(v), now);
                    return;
                };
                if let Some(program) = echo {
                    self.config.pc_offset = Some(v - i32::from(program));
                }
                let moved = match button {
                    Button::Next => cur > from,
                    Button::Previous => cur < from,
                };
                sel.failed = if moved {
                    0
                } else {
                    sel.failed.saturating_add(1)
                };
                self.decide(sel, cur, now);
            }
            (
                SelectPhase::Start | SelectPhase::PcVerify { .. } | SelectPhase::StepVerify { .. },
                _,
            ) => {
                self.select_read_missed(now);
            }
            (SelectPhase::Name { number }, reply) => {
                let name = match reply {
                    Reply::Name(name) => Some(name),
                    _ => None,
                };
                if sel.target == number {
                    self.set_current(number, name);
                    self.notice(Notice::Reached { number });
                } else {
                    // A new target arrived while reading the name.
                    self.current = Some(Preset { number, name });
                    self.decide(sel, number, now);
                }
            }
        }
    }

    /// At preset `cur`: done, try Program Change, step, or give up.
    fn decide(&mut self, mut sel: Select, cur: u16, now: u64) {
        sel.at = Some(cur);
        let target = sel.target;
        if cur == target {
            sel.phase = SelectPhase::Name { number: cur };
            self.job = Job::Select(sel);
            self.request_name(now);
            return;
        }
        if !sel.tried_pc
            && self.config.program_change != ProgramChangeSupport::Ignored
            && let Some(offset) = self.config.pc_offset
            && let Ok(program) = u8::try_from(i32::from(target) - offset)
            && program <= 127
        {
            sel.tried_pc = true;
            sel.phase = SelectPhase::PcSettle { sent_for: target };
            self.job = Job::Select(sel);
            self.send(Request::ProgramChange {
                channel: self.pc_channel,
                program,
            });
            self.expect(Expect::Timer, now + self.config.program_change_settle_ms);
            return;
        }
        if sel.failed >= self.config.max_failed_steps {
            self.fail_select(FailReason::NoProgress { at: cur, target }, now);
            return;
        }
        if sel.steps >= self.config.max_steps {
            self.fail_select(FailReason::TooManySteps { at: cur, target }, now);
            return;
        }
        let button = if target > cur {
            Button::Next
        } else {
            Button::Previous
        };
        self.notice(Notice::Stepping {
            from: cur,
            to: target,
        });
        self.send(Request::Button {
            button,
            press: true,
        });
        self.release = Some((now + self.config.press_ms, button));
        sel.steps += 1;
        sel.phase = SelectPhase::StepEcho { from: cur, button };
        self.job = Job::Select(sel);
        self.expect(
            Expect::Echo,
            now + self.config.press_ms + self.config.step_timeout_ms,
        );
    }
}

fn hi7(v: u16) -> u8 {
    ((v >> 7) & 0x7F) as u8
}

fn lo7(v: u16) -> u8 {
    (v & 0x7F) as u8
}

/// A 28-bit value as four 7-bit bytes, big-endian; negative values in two's complement.
#[must_use]
pub fn encode_28(value: i32) -> [u8; 4] {
    let v = value.rem_euclid(1 << 28) as u32;
    [
        ((v >> 21) & 0x7F) as u8,
        ((v >> 14) & 0x7F) as u8,
        ((v >> 7) & 0x7F) as u8,
        (v & 0x7F) as u8,
    ]
}

/// The 28-bit value in the first four bytes of `bytes`.
#[must_use]
pub fn decode_28(bytes: &[u8]) -> Option<i32> {
    let [a, b, c, d, ..] = *bytes else {
        return None;
    };
    let v = i32::from(a & 0x7F) << 21
        | i32::from(b & 0x7F) << 14
        | i32::from(c & 0x7F) << 7
        | i32::from(d & 0x7F);
    Some(if v >= 1 << 27 { v - (1 << 28) } else { v })
}

/// The value of `param` in a `22` reply (`F0 00 01 38 <ch> <product> 22 p1 p0 v3 v2 v1 v0 F7`).
#[must_use]
pub fn param_reply(m: &[u8], param: u16) -> Option<i32> {
    let [
        0xF0,
        0x00,
        0x01,
        0x38,
        _,
        _,
        command::SET_PARAM,
        p1,
        p0,
        ref value @ ..,
        0xF7,
    ] = *m
    else {
        return None;
    };
    if p1 != hi7(param) || p0 != lo7(param) || value.len() != 4 {
        return None;
    }
    decode_28(value)
}

/// The name in the preset header the pedal sends for `45 00 00` (the
/// current preset): bytes 11–25, ending at the first `00`.
#[must_use]
pub fn current_preset_name(m: &[u8]) -> Option<String> {
    if m.len() < 31 {
        return None;
    }
    let [
        0xF0,
        0x00,
        0x01,
        0x38,
        _,
        _,
        command::PRESET_HEADER,
        0x00,
        0x00,
        ..,
    ] = *m
    else {
        return None;
    };
    Some(preset_name(m))
}

/// The name in a preset header message (bytes 11–25, ending at the first `00`;
/// printable ASCII only, trimmed).
#[must_use]
pub fn preset_name(m: &[u8]) -> String {
    let raw = m.get(11..m.len().min(26)).unwrap_or_default();
    let name: String = raw
        .iter()
        .take_while(|b| **b != 0)
        .filter(|b| (0x20..0x7F).contains(*b))
        .map(|b| char::from(*b))
        .collect();
    name.trim().to_string()
}

fn program_change(m: &[u8]) -> Option<(u8, u8)> {
    match *m {
        [status, program] if status & 0xF0 == 0xC0 && program <= 0x7F => {
            Some((status & 0x0F, program))
        }
        _ => None,
    }
}

/// Whether `msg` is a Play Acoustic SysEx that changes something on the
/// device: set parameter, setup, preset write, preset delete or a sync
/// session. The driver never sends any of these.
#[must_use]
pub fn is_device_write(msg: &[u8]) -> bool {
    matches!(
        *msg,
        [0xF0, 0x00, 0x01, 0x38, _, _, c, ..] if matches!(
            c,
            command::SET_PARAM
                | command::SETUP
                | command::PRESET_HEADER
                | command::PRESET_DATA
                | command::DELETE_PRESET
                | command::SYNC_SESSION
        )
    )
}

/// A stand-in for the pedal that answers like the firmware: identity, `47`
/// reads of `0x2A4`, `45` preset names, the `55` panel tunnel; ▶ / ◀ step
/// presets and are confirmed with Bank Select + Program Change; Program
/// Change itself is dropped unless [`accepts_program_change`](Self::accepts_program_change).
///
/// For tests and the app's "simulated pedal" mode: hand it what the driver
/// sends ([`receive`](Self::receive)) and give the driver its replies.
#[derive(Debug, Clone)]
pub struct SimulatedPlayAcoustic {
    /// The current preset.
    pub preset: u16,
    /// Presets stored on the pedal.
    pub range: RangeInclusive<u16>,
    /// Preset names; others are called "Preset n".
    pub names: BTreeMap<u16, String>,
    pub model: Model,
    /// Whether Program Change selects a preset (false = like the real unit over USB).
    pub accepts_program_change: bool,
    /// No replies at all, as while the pedal restarts behind a port that stays.
    pub silent: bool,
    /// A menu is open: ▶ / ◀ change pages instead of presets.
    pub in_menu: bool,
    tunnel: bool,
    parser: StreamParser,
    received: Vec<Vec<u8>>,
}

/// How many received messages the simulation keeps for inspection.
const SIM_LOG_LIMIT: usize = 4096;

impl SimulatedPlayAcoustic {
    /// A Play Acoustic at `preset` with presets `range`.
    #[must_use]
    pub fn new(preset: u16, range: RangeInclusive<u16>) -> Self {
        Self {
            preset,
            range,
            names: BTreeMap::new(),
            model: Model::PlayAcoustic,
            accepts_program_change: false,
            silent: false,
            in_menu: false,
            tunnel: false,
            parser: StreamParser::with_max_sysex_len(4096),
            received: Vec::new(),
        }
    }

    /// The same with preset names.
    #[must_use]
    pub fn with_names<'a>(mut self, names: impl IntoIterator<Item = (u16, &'a str)>) -> Self {
        self.names
            .extend(names.into_iter().map(|(n, s)| (n, s.to_string())));
        self
    }

    /// Whether the front-panel tunnel is on (injected events act only then).
    #[must_use]
    pub fn tunnel_on(&self) -> bool {
        self.tunnel
    }

    /// Every message received (the most recent ones, if very many).
    #[must_use]
    pub fn received(&self) -> &[Vec<u8>] {
        &self.received
    }

    /// Restarts like a power cycle: the tunnel is off afterwards.
    pub fn restart(&mut self) {
        self.tunnel = false;
        self.parser.reset();
    }

    /// Changes the preset as if a footswitch on the unit was pressed; returns
    /// what the pedal sends.
    pub fn footswitch(&mut self, up: bool) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        self.step(up, &mut out);
        self.filter(out)
    }

    /// Takes bytes from the driver and returns the pedal's replies.
    pub fn receive(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();
        self.parser.feed(bytes, |m| messages.push(m.to_vec()));
        let mut out = Vec::new();
        for m in messages {
            self.answer(&m, &mut out);
            if self.received.len() >= SIM_LOG_LIMIT {
                self.received.drain(..SIM_LOG_LIMIT / 2);
            }
            self.received.push(m);
        }
        self.filter(out)
    }

    fn filter(&self, out: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        if self.silent { Vec::new() } else { out }
    }

    fn header(&self) -> Vec<u8> {
        vec![0xF0, 0x00, 0x01, 0x38, 0x00, self.model.product_code()]
    }

    fn step(&mut self, up: bool, out: &mut Vec<Vec<u8>>) {
        let next = if up {
            self.preset.checked_add(1)
        } else {
            self.preset.checked_sub(1)
        };
        if let Some(next) = next.filter(|n| self.range.contains(n)) {
            self.preset = next;
            out.push(vec![0xB0, 0x00, 0x00]);
            out.push(vec![0xC0, (next.saturating_sub(1) & 0x7F) as u8]);
        }
    }

    fn answer(&mut self, m: &[u8], out: &mut Vec<Vec<u8>>) {
        if let Some((_, program)) = program_change(m) {
            let n = u16::from(program) + 1;
            if self.accepts_program_change && self.range.contains(&n) {
                self.preset = n;
                out.push(vec![0xC0, program]);
            }
            return;
        }
        if let [0xF0, 0x7E, _, 0x06, 0x01, 0xF7] = *m {
            out.push(vec![
                0xF0,
                0x7E,
                0x00,
                0x06,
                0x02,
                0x00,
                0x01,
                0x38,
                self.model.product_code(),
                0x00,
                0x01,
                0x04,
                0x00,
                0xF7,
            ]);
            return;
        }
        let [0xF0, 0x00, 0x01, 0x38, _, product, cmd, ref rest @ ..] = *m else {
            return;
        };
        if product != self.model.product_code() && product != 0x7F {
            return;
        }
        match (cmd, rest) {
            (command::GET_PARAM, [p1, p0, 0xF7]) => {
                let id = u16::from(*p1) << 7 | u16::from(*p0);
                let value = if id == PRESET_PARAM {
                    i32::from(self.preset)
                } else {
                    0
                };
                let mut r = self.header();
                r.extend([command::SET_PARAM, *p1, *p0]);
                r.extend(encode_28(value));
                r.push(0xF7);
                out.push(r);
            }
            (command::REQUEST_PRESET, [hi, lo, 0xF7]) => {
                // 45 00 00 = the current preset, 45 <n> = stored preset n without loading it
                let n = u16::from(*hi) << 7 | u16::from(*lo);
                let which = if n == 0 { self.preset } else { n };
                if n != 0 && !self.range.contains(&n) {
                    let mut r = self.header();
                    r.extend([0x34, 0x02, 0x00, 0x00, 0xF7]);
                    out.push(r);
                    return;
                }
                let name = self
                    .names
                    .get(&which)
                    .cloned()
                    .unwrap_or_else(|| format!("Preset {which}"));
                let mut name: Vec<u8> = name
                    .bytes()
                    .map(|b| if (0x20..0x7F).contains(&b) { b } else { b'?' })
                    .take(15)
                    .collect();
                name.resize(15, 0);
                let mut r = self.header();
                r.extend([command::PRESET_HEADER, *hi, *lo, 0x01, 0x0C]);
                r.extend(name);
                r.extend([0, 0, 0, 0, 0x01, 0xF7]);
                out.push(r);
                for i in 0..5u8 {
                    let mut d = self.header();
                    d.extend([command::PRESET_DATA, i, 0]);
                    d.extend([0u8; 100]);
                    d.push(0xF7);
                    out.push(d);
                }
            }
            (command::TUNNEL, [0x01, on, _, _, _, 0xF7]) => self.tunnel = *on != 0,
            (command::TUNNEL, [0x02, a, b, c, d, 0xF7]) => {
                if !self.tunnel {
                    return;
                }
                let Some(event) = decode_28(&[*a, *b, *c, *d]) else {
                    return;
                };
                let mut echo = self.header();
                echo.push(command::TUNNEL);
                echo.extend(encode_28(event));
                echo.push(0xF7);
                out.push(echo);
                let (e0, e1, e2) = ((event >> 16) & 0xFF, (event >> 8) & 0xFF, event & 0xFF);
                if e0 == 0xDB && e2 == 0x00 && !self.in_menu {
                    match e1 {
                        0x04 | 0x0C => self.step(true, out),
                        0x05 | 0x0D => self.step(false, out),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_name_decoding() {
        let mut m = vec![0xF0, 0, 1, 0x38, 0, 0x70, 0x20, 0, 0, 0, 0];
        m.extend(b"Big Chorus");
        m.extend([0, 0x41, 0x42, 0, 0]);
        m.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xF7]);
        assert_eq!(current_preset_name(&m).as_deref(), Some("Big Chorus"));
        assert_eq!(preset_name(&[0xF0, 0xF7]), "");
        assert_eq!(current_preset_name(&m[..20]), None);
    }

    #[test]
    fn values_in_28_bits() {
        assert_eq!(decode_28(&encode_28(-5)), Some(-5));
        assert_eq!(decode_28(&encode_28(0xDB_0400)), Some(0xDB_0400));
        assert_eq!(encode_28(0x2A4), [0, 0, 0x05, 0x24]);
        assert_eq!(decode_28(&[1, 2, 3]), None);
        assert_eq!(decode_28(&encode_28(i32::MIN)), Some(0));
    }

    #[test]
    fn parameter_replies() {
        let m = [
            0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x22, 0x05, 0x24, 0x00, 0x00, 0x00, 0x0C, 0xF7,
        ];
        assert_eq!(param_reply(&m, PRESET_PARAM), Some(12));
        assert_eq!(param_reply(&m, 0x158), None);
        assert_eq!(param_reply(&m[..13], PRESET_PARAM), None);
        assert!(is_device_write(&m), "22 sent to the pedal is a write");
        assert!(!is_device_write(&[
            0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x47, 0x05, 0x24, 0xF7
        ]));
    }

    #[test]
    fn requests_are_reads_and_events_only() {
        let mut d = Driver::new(Config::default());
        d.tunnel_on = true;
        d.channel = 0;
        for r in [
            Request::Identity,
            Request::GetParam(PRESET_PARAM),
            Request::CurrentPresetName,
            Request::TunnelOn,
            Request::Button {
                button: Button::Next,
                press: true,
            },
            Request::Button {
                button: Button::Previous,
                press: false,
            },
            Request::ProgramChange {
                channel: 0,
                program: 5,
            },
        ] {
            d.send(r);
        }
        let sent: Vec<Vec<u8>> = d
            .take()
            .into_iter()
            .filter_map(|o| match o {
                Output::Send(b) => Some(b),
                Output::Notice(_) => None,
            })
            .collect();
        assert_eq!(
            sent,
            vec![
                vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7],
                vec![0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x47, 0x05, 0x24, 0xF7],
                vec![0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x45, 0x00, 0x00, 0xF7],
                vec![
                    0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x55, 0x01, 0x01, 0x00, 0x00, 0x00, 0xF7
                ],
                vec![
                    0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x55, 0x02, 0x06, 0x6C, 0x08, 0x00, 0xF7
                ],
                vec![
                    0xF0, 0x00, 0x01, 0x38, 0x00, 0x70, 0x55, 0x02, 0x06, 0x6C, 0x0A, 0x01, 0xF7
                ],
                vec![0xC0, 0x05],
            ]
        );
        assert!(!sent.iter().any(|m| is_device_write(m)));
    }
}
