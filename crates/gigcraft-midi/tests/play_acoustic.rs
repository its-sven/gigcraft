//! The Play Acoustic driver against the simulated pedal, with simulated time.
//! Mirrors Cuebook's pedal tests.

use gigcraft_midi::play_acoustic::{
    Config, Driver, FailReason, Model, Notice, Output, ProgramChangeSupport, SimulatedPlayAcoustic,
    State, command, is_device_write,
};
use std::collections::VecDeque;

/// Driver and simulated pedal connected by a wire with a little latency.
struct Rig {
    driver: Driver,
    pedal: SimulatedPlayAcoustic,
    now: u64,
    latency: u64,
    /// Deliver the pedal's replies one byte at a time.
    split_bytes: bool,
    wire: VecDeque<(u64, Vec<u8>)>,
    notices: Vec<Notice>,
}

impl Rig {
    fn new(pedal: SimulatedPlayAcoustic) -> Self {
        Self::with_config(pedal, Config::default())
    }

    fn with_config(pedal: SimulatedPlayAcoustic, config: Config) -> Self {
        Self {
            driver: Driver::new(config),
            pedal,
            now: 0,
            latency: 2,
            split_bytes: false,
            wire: VecDeque::new(),
            notices: Vec::new(),
        }
    }

    fn apply(&mut self, outputs: Vec<Output>) {
        for o in outputs {
            match o {
                Output::Send(bytes) => {
                    for reply in self.pedal.receive(&bytes) {
                        self.wire.push_back((self.now + self.latency, reply));
                    }
                }
                Output::Notice(n) => self.notices.push(n),
            }
        }
    }

    /// Runs until nothing is left to do or `max_ms` have passed.
    fn run(&mut self, max_ms: u64) {
        let end = self.now + max_ms;
        for _ in 0..1_000_000 {
            let next = [
                self.wire.front().map(|(t, _)| *t),
                self.driver.next_deadline(),
            ]
            .into_iter()
            .flatten()
            .min();
            let Some(next) = next else { break };
            if next > end {
                break;
            }
            self.now = self.now.max(next);
            while self.wire.front().is_some_and(|(t, _)| *t <= self.now) {
                let Some((_, bytes)) = self.wire.pop_front() else {
                    break;
                };
                let out = if self.split_bytes {
                    let mut all = Vec::new();
                    for b in bytes {
                        all.extend(self.driver.on_message(&[b], self.now));
                    }
                    all
                } else {
                    self.driver.on_message(&bytes, self.now)
                };
                self.apply(out);
            }
            let out = self.driver.tick(self.now);
            self.apply(out);
        }
        self.now = self.now.max(end);
    }

    fn connect(&mut self) {
        let out = self.driver.connect(self.now);
        self.apply(out);
        self.run(10_000);
    }

    fn select(&mut self, target: u16) {
        let out = self.driver.select_preset(target, self.now);
        self.apply(out);
    }

    fn go(&mut self, target: u16) {
        self.notices.clear();
        self.select(target);
        self.run(60_000);
    }

    fn read(&mut self) {
        let out = self.driver.read_current(self.now);
        self.apply(out);
        self.run(10_000);
    }

    fn program_changes_sent(&self) -> usize {
        self.pedal
            .received()
            .iter()
            .filter(|m| m.len() == 2 && m[0] & 0xF0 == 0xC0)
            .count()
    }

    fn button_presses_sent(&self) -> usize {
        self.pedal
            .received()
            .iter()
            .filter(|m| m.len() == 13 && m[6] == command::TUNNEL && m[7] == 0x02 && m[11] == 0x00)
            .count()
    }

    fn reached(&self) -> Option<u16> {
        self.notices.iter().find_map(|n| match n {
            Notice::Reached { number } => Some(*number),
            _ => None,
        })
    }

    fn failures(&self) -> Vec<FailReason> {
        self.notices
            .iter()
            .filter_map(|n| match n {
                Notice::Failed { reason } => Some(reason.clone()),
                _ => None,
            })
            .collect()
    }
}

fn pedal(preset: u16) -> SimulatedPlayAcoustic {
    SimulatedPlayAcoustic::new(preset, 1..=100)
}

#[test]
fn connect_reads_identity_and_preset() {
    let mut rig = Rig::new(pedal(3).with_names([(3, "Coffee House Du")]));
    rig.connect();
    assert_eq!(
        rig.driver.state(),
        State::Ready {
            model: Model::PlayAcoustic
        }
    );
    assert_eq!(
        rig.notices,
        vec![
            Notice::Connected {
                model: Model::PlayAcoustic
            },
            Notice::CurrentPreset {
                number: 3,
                name: Some("Coffee House Du".into())
            },
        ]
    );
    assert_eq!(rig.driver.current().map(|p| p.number), Some(3));
    assert!(rig.pedal.tunnel_on(), "front-panel tunnel switched on");
    assert!(!rig.driver.is_busy());
    assert_eq!(rig.driver.next_deadline(), None);
}

#[test]
fn play_electric_is_driven_with_its_own_header() {
    let mut p = pedal(2);
    p.model = Model::PlayElectric;
    let mut rig = Rig::new(p);
    rig.connect();
    assert_eq!(
        rig.notices.first(),
        Some(&Notice::Connected {
            model: Model::PlayElectric
        })
    );
    rig.go(4);
    assert_eq!(rig.reached(), Some(4));
    assert!(
        rig.pedal
            .received()
            .iter()
            .filter(|m| m.first() == Some(&0xF0) && m.get(1) == Some(&0x00))
            .all(|m| m[5] == 0x71)
    );
}

#[test]
fn select_steps_with_the_buttons() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.go(7);
    assert_eq!(rig.reached(), Some(7));
    assert_eq!(rig.pedal.preset, 7);
    assert_eq!(rig.driver.current().map(|p| p.number), Some(7));
    assert_eq!(rig.button_presses_sent(), 4);
    let steps: Vec<(u16, u16)> = rig
        .notices
        .iter()
        .filter_map(|n| match n {
            Notice::Stepping { from, to } => Some((*from, *to)),
            _ => None,
        })
        .collect();
    assert_eq!(steps, [(3, 7), (4, 7), (5, 7), (6, 7)]);
    assert!(rig.notices.contains(&Notice::CurrentPreset {
        number: 7,
        name: Some("Preset 7".into())
    }));

    rig.go(5);
    assert_eq!(rig.reached(), Some(5));
    assert_eq!(rig.pedal.preset, 5);
    assert!(rig.failures().is_empty());
}

#[test]
fn every_press_is_released() {
    let mut rig = Rig::new(pedal(10));
    rig.connect();
    rig.go(4);
    let events: Vec<u8> = rig
        .pedal
        .received()
        .iter()
        .filter(|m| m.len() == 13 && m[6] == command::TUNNEL && m[7] == 0x02)
        .map(|m| m[11])
        .collect();
    assert_eq!(events.len(), 12);
    assert!(
        events.chunks(2).all(|pair| pair == [0x00, 0x01]),
        "{events:?}"
    );
}

#[test]
fn already_there_needs_no_step() {
    let mut rig = Rig::new(pedal(9));
    rig.connect();
    rig.go(9);
    assert_eq!(rig.reached(), Some(9));
    assert_eq!(rig.button_presses_sent(), 0);
}

#[test]
fn program_change_is_tried_once_then_remembered() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.go(4); // one step: learns the Program Change offset from the pedal's own message
    assert_eq!(rig.driver.pc_offset(), Some(1));
    assert_eq!(rig.program_changes_sent(), 0);
    rig.go(9); // tries Program Change (dropped), then steps
    assert_eq!(rig.pedal.preset, 9);
    assert_eq!(rig.reached(), Some(9));
    assert_eq!(rig.program_changes_sent(), 1);
    assert_eq!(rig.driver.program_change(), ProgramChangeSupport::Ignored);
    assert!(
        rig.notices
            .contains(&Notice::ProgramChangeTried { works: false })
    );
    rig.go(6);
    assert_eq!(
        rig.program_changes_sent(),
        1,
        "no more Program Change once known not to work"
    );
    assert_eq!(rig.pedal.preset, 6);

    // A new session that remembers it never tries again either.
    let config = Config {
        program_change: rig.driver.program_change(),
        pc_offset: rig.driver.pc_offset(),
        ..Config::default()
    };
    let mut again = Rig::with_config(pedal(3), config);
    again.connect();
    again.go(8);
    assert_eq!(again.program_changes_sent(), 0);
    assert_eq!(again.pedal.preset, 8);
}

#[test]
fn program_change_is_used_when_the_pedal_takes_it() {
    let mut p = pedal(3);
    p.accepts_program_change = true;
    let mut rig = Rig::new(p);
    rig.connect();
    rig.go(4); // one step, learns the offset
    let presses = rig.button_presses_sent();
    rig.go(40);
    assert_eq!(rig.reached(), Some(40));
    assert_eq!(rig.pedal.preset, 40);
    assert_eq!(
        rig.button_presses_sent(),
        presses,
        "jumped without button presses"
    );
    assert_eq!(rig.driver.program_change(), ProgramChangeSupport::Works);
    rig.go(2);
    assert_eq!(rig.button_presses_sent(), presses);
    assert_eq!(rig.pedal.preset, 2);
}

/// Next pressed twice quickly: the second request redirects the walk instead of failing.
#[test]
fn a_second_request_while_stepping_changes_the_target() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.notices.clear();
    rig.select(9);
    rig.run(20);
    assert!(rig.driver.is_busy());
    assert!(rig.pedal.preset < 6);
    rig.select(6);
    assert_eq!(rig.driver.target(), Some(6));
    rig.run(60_000);
    assert_eq!(rig.reached(), Some(6));
    assert_eq!(rig.pedal.preset, 6);
    assert_eq!(rig.driver.current().map(|p| p.number), Some(6));
    assert!(rig.failures().is_empty());
    assert_eq!(
        rig.notices
            .iter()
            .filter(|n| matches!(n, Notice::Reached { .. }))
            .count(),
        1
    );

    // and backwards, changing direction mid-way
    rig.notices.clear();
    rig.select(40);
    rig.run(30);
    assert!(rig.driver.is_busy() && rig.pedal.preset > 6);
    rig.select(2);
    rig.run(60_000);
    assert_eq!(rig.reached(), Some(2));
    assert_eq!(rig.pedal.preset, 2);
}

#[test]
fn a_request_during_a_read_waits_for_it() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.notices.clear();
    let out = rig.driver.read_current(rig.now);
    rig.apply(out);
    rig.select(5);
    rig.run(60_000);
    assert_eq!(rig.reached(), Some(5));
    assert!(rig.notices.contains(&Notice::CurrentPreset {
        number: 3,
        name: Some("Preset 3".into())
    }));
}

#[test]
fn never_writes_to_the_pedal() {
    let mut p = pedal(3);
    p.accepts_program_change = true;
    let mut rig = Rig::new(p);
    rig.connect();
    rig.go(6);
    rig.go(2);
    rig.read();
    rig.select(30);
    rig.run(5);
    rig.select(1);
    rig.run(60_000);
    let footswitch = rig.pedal.footswitch(true);
    for m in footswitch {
        let out = rig.driver.on_message(&m, rig.now);
        rig.apply(out);
    }
    rig.run(10_000);
    let sent = rig.pedal.received();
    assert!(sent.len() > 20);
    assert!(
        !sent.iter().any(|m| is_device_write(m)),
        "no write of any kind"
    );
    for m in sent {
        if m.len() > 6 && m[..4] == [0xF0, 0x00, 0x01, 0x38] {
            assert!(
                command::SENT_BY_DRIVER.contains(&m[6]),
                "only reads and panel events: {m:02X?}"
            );
            assert_ne!(m[6], command::SET_PARAM);
        }
    }
}

#[test]
fn footswitch_on_the_pedal_updates_current() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.notices.clear();
    let msgs = rig.pedal.footswitch(true);
    for m in msgs {
        let out = rig.driver.on_message(&m, rig.now);
        rig.apply(out);
    }
    rig.run(10_000);
    assert_eq!(rig.driver.current().map(|p| p.number), Some(4));
    assert_eq!(
        rig.notices,
        vec![Notice::CurrentPreset {
            number: 4,
            name: Some("Preset 4".into())
        }]
    );
}

#[test]
fn no_answer_from_other_devices() {
    let mut p = pedal(3);
    p.silent = true;
    let mut rig = Rig::new(p);
    rig.connect();
    assert_eq!(rig.driver.state(), State::NotSupported);
    assert_eq!(rig.notices, vec![Notice::NotSupported { identity: None }]);
    assert_eq!(
        rig.pedal.received().len(),
        3,
        "three identity requests, nothing else"
    );
    rig.notices.clear();
    rig.go(5);
    assert_eq!(rig.failures(), vec![FailReason::NotConnected]);
    assert_eq!(rig.pedal.received().len(), 3);
}

#[test]
fn another_device_is_not_driven() {
    let mut driver = Driver::new(Config::default());
    let out = driver.connect(0);
    assert_eq!(
        out,
        vec![Output::Send(vec![0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7])]
    );
    // A Roland answers.
    let reply = [
        0xF0, 0x7E, 0x10, 0x06, 0x02, 0x41, 0x0B, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0xF7,
    ];
    let out = driver.on_message(&reply, 5);
    assert!(matches!(
        out.as_slice(),
        [Output::Notice(Notice::NotSupported { identity: Some(id) })] if id.manufacturer == [0x41]
    ));
    assert_eq!(driver.state(), State::NotSupported);
    assert_eq!(
        driver.select_preset(3, 10),
        vec![Output::Notice(Notice::Failed {
            reason: FailReason::NotConnected
        })]
    );
    assert_eq!(driver.next_deadline(), None);
}

#[test]
fn gives_up_when_the_pedal_is_in_a_menu() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.pedal.in_menu = true;
    rig.go(7);
    assert_eq!(
        rig.failures(),
        vec![FailReason::NoProgress { at: 3, target: 7 }]
    );
    assert_eq!(rig.reached(), None);
    assert_eq!(
        rig.button_presses_sent(),
        3,
        "bounded number of failed steps"
    );
    assert!(!rig.driver.is_busy());
    let message = rig.failures()[0].to_string();
    assert!(message.contains("preset screen"), "{message}");
    // Back on the preset screen, it works again.
    rig.pedal.in_menu = false;
    rig.go(7);
    assert_eq!(rig.reached(), Some(7));
}

#[test]
fn gives_up_at_the_last_preset() {
    let mut rig = Rig::new(SimulatedPlayAcoustic::new(10, 1..=12));
    rig.connect();
    rig.go(20);
    assert_eq!(
        rig.failures(),
        vec![FailReason::NoProgress { at: 12, target: 20 }]
    );
    assert_eq!(rig.pedal.preset, 12);
    assert_eq!(rig.driver.current().map(|p| p.number), Some(12));
}

#[test]
fn step_limit_and_preset_range() {
    let config = Config {
        max_steps: 2,
        max_preset: Some(50),
        ..Config::default()
    };
    let mut rig = Rig::with_config(pedal(3), config);
    rig.connect();
    rig.go(9);
    assert_eq!(
        rig.failures(),
        vec![FailReason::TooManySteps { at: 5, target: 9 }]
    );
    rig.go(51);
    assert_eq!(rig.failures(), vec![FailReason::OutOfRange { target: 51 }]);
    rig.go(0);
    assert_eq!(rig.failures(), vec![FailReason::OutOfRange { target: 0 }]);
}

/// The pedal restarts behind a port that stays: the driver notices, checks
/// again and switches the tunnel on, so stepping works afterwards.
#[test]
fn recovers_after_the_pedal_restarts() {
    let config = Config {
        retry_interval_ms: 200,
        ..Config::default()
    };
    let mut rig = Rig::with_config(pedal(3), config);
    rig.connect();
    assert!(rig.pedal.tunnel_on());
    rig.pedal.silent = true;
    rig.pedal.restart();
    rig.notices.clear();
    for _ in 0..3 {
        rig.read();
    }
    assert_eq!(
        rig.driver.state(),
        State::Lost {
            model: Model::PlayAcoustic
        }
    );
    assert!(rig.notices.contains(&Notice::Lost));
    rig.notices.clear();
    rig.go(5);
    assert_eq!(rig.failures(), vec![FailReason::NotConnected]);

    rig.pedal.silent = false;
    rig.notices.clear();
    rig.run(5_000);
    assert!(matches!(rig.driver.state(), State::Ready { .. }));
    assert!(rig.notices.contains(&Notice::Connected {
        model: Model::PlayAcoustic
    }));
    assert!(rig.pedal.tunnel_on(), "tunnel switched on again");
    rig.go(5);
    assert_eq!(rig.reached(), Some(5));
    assert_eq!(rig.pedal.preset, 5);
}

#[test]
fn losing_the_pedal_while_stepping_fails_the_selection() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.notices.clear();
    rig.select(9);
    rig.run(30);
    rig.pedal.silent = true;
    rig.run(10_000);
    let failures = rig.failures();
    assert_eq!(failures.len(), 1, "{:?}", rig.notices);
    assert!(
        matches!(failures[0], FailReason::NoAnswer | FailReason::Lost),
        "{failures:?}"
    );
    assert!(!rig.driver.is_busy());
}

#[test]
fn replies_split_into_single_bytes() {
    let mut rig = Rig::new(pedal(3).with_names([(6, "Warm Ballad")]));
    rig.split_bytes = true;
    rig.connect();
    rig.go(6);
    assert_eq!(rig.reached(), Some(6));
    assert!(rig.notices.contains(&Notice::CurrentPreset {
        number: 6,
        name: Some("Warm Ballad".into())
    }));
}

#[test]
fn bluetooth_timings_work_with_slow_links() {
    let mut rig = Rig::with_config(pedal(3), Config::bluetooth());
    rig.latency = 400;
    rig.connect();
    rig.go(5);
    assert_eq!(rig.reached(), Some(5));
}

#[test]
fn disconnect_ends_a_selection() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.notices.clear();
    rig.select(9);
    rig.run(10);
    let out = rig.driver.disconnect();
    rig.apply(out);
    assert_eq!(rig.driver.state(), State::Idle);
    assert_eq!(rig.failures(), vec![FailReason::NotConnected]);
    assert_eq!(rig.driver.next_deadline(), None);
}

#[test]
fn garbage_and_late_replies_never_panic() {
    let mut rig = Rig::new(pedal(3));
    rig.connect();
    rig.select(50);
    let mut x: u32 = 0xC0FF_EE11;
    for i in 0..20_000u64 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        let len = (x % 5) as usize + 1;
        let bytes: Vec<u8> = (0..len).map(|k| (x >> (k * 5)) as u8).collect();
        let out = rig.driver.on_message(&bytes, rig.now + i);
        rig.apply(out);
        if i % 97 == 0 {
            let out = rig.driver.tick(rig.now + i);
            rig.apply(out);
        }
    }
    rig.now += 20_000;
    rig.run(120_000);
    // Whatever happened, the driver ends in a defined state and is usable.
    rig.go(7);
    assert!(rig.reached() == Some(7) || !rig.failures().is_empty());
}
