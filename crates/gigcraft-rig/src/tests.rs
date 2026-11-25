use super::*;
use gigcraft_model::MidiMessage;

fn pc(device: &str, program: u8) -> MidiAction {
    MidiAction {
        device: device.into(),
        message: MidiMessage::ProgramChange {
            channel: 1,
            program,
            bank_msb: None,
            bank_lsb: None,
        },
    }
}

/// Run ticks until the rig is idle (simulated time, 10 ms steps).
fn settle(rig: &mut Rig, mut now: u64, out: &mut Vec<RigOut>) -> u64 {
    for _ in 0..20_000 {
        if rig.next_deadline().is_none() {
            break;
        }
        now += 10;
        out.extend(rig.tick(now));
    }
    now
}

fn texts(out: &[RigOut]) -> Vec<String> {
    out.iter()
        .filter_map(|o| match o {
            RigOut::Notice { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn simulated_play_acoustic_reaches_the_preset() {
    let mut out = Vec::new();
    let mut rig = Rig::new(RigConfig::demo());
    let mut now = settle(&mut rig, 0, &mut out);
    assert!(
        rig.status()[0].state.contains("ready"),
        "{:?}",
        rig.status()
    );
    // Wire program 3 = preset 4 on the pedal (numbering from 1).
    out.extend(rig.send_part(&[pc("play acoustic", 3)], now));
    now = settle(&mut rig, now, &mut out);
    let t = texts(&out);
    assert!(t.iter().any(|x| x == "at preset 4"), "{t:?}");
    assert_eq!(rig.status()[0].current.as_deref(), Some("4 Bluesy Drive"));
    assert!(
        out.iter().any(|o| matches!(o, RigOut::ConfigChanged)),
        "PC result is persisted"
    );
    assert_eq!(
        rig.config().devices[0].program_change.as_deref(),
        Some("ignored")
    );
    assert!(
        !out.iter().any(|o| matches!(o, RigOut::Send { .. })),
        "simulated: nothing goes to a port"
    );
    let _ = now;
}

#[test]
fn generic_device_sends_program_change_to_its_port() {
    let cfg = RigConfig {
        devices: vec![DeviceConfig {
            name: "Helix".into(),
            port: Some("helix".into()),
            profile: profile::GENERIC_MIDI.into(),
            simulated: false,
            program_change: None,
            pc_offset: None,
        }],
        mappings: Vec::new(),
        gap_ms: 0,
    };
    let mut rig = Rig::new(cfg);
    let out = rig.send_part(&[pc("Helix", 7)], 0);
    assert_eq!(texts(&out), ["no MIDI port; preset not sent"]);
    rig.ports_changed(&["IAC Bus 1".into(), "Line 6 HELIX".into()], 0);
    let out = rig.send_part(&[pc("Helix", 7), pc("Nobody", 1)], 0);
    assert_eq!(
        out[0],
        RigOut::Notice {
            device: "Nobody".into(),
            level: Level::Warning,
            text: "no device with this name in the rig settings".into()
        }
    );
    assert_eq!(
        out[1],
        RigOut::Send {
            port: "Line 6 HELIX".into(),
            bytes: vec![0xC0, 7]
        }
    );
    let panic = rig.panic(0);
    assert_eq!(panic.len(), 32);
}

#[test]
fn gap_delays_later_messages() {
    let mut cfg = RigConfig::default();
    cfg.devices.push(DeviceConfig {
        name: "Synth".into(),
        port: Some("synth".into()),
        profile: profile::GENERIC_MIDI.into(),
        simulated: false,
        program_change: None,
        pc_offset: None,
    });
    cfg.gap_ms = 50;
    let mut rig = Rig::new(cfg);
    rig.ports_changed(&["Synth".into()], 0);
    let out = rig.send_part(&[pc("Synth", 1), pc("Synth", 2)], 1000);
    assert_eq!(out.len(), 1);
    assert_eq!(rig.next_deadline(), Some(1050));
    assert!(rig.tick(1049).is_empty());
    assert_eq!(
        rig.tick(1050),
        vec![RigOut::Send {
            port: "Synth".into(),
            bytes: vec![0xC0, 2]
        }]
    );
}

#[test]
fn learn_and_map_midi_input_to_actions() {
    let mut rig = Rig::new(RigConfig::default());
    rig.learn(true);
    let out = rig.on_input("FS-6", &[0xB0, 80, 127], 0);
    let RigOut::Learned { trigger, .. } = &out[0] else {
        panic!("{out:?}")
    };
    rig.add_mapping(Mapping {
        trigger: trigger.clone(),
        action: Action::Next,
        port: None,
    });
    assert!(!rig.is_learning());
    // Press (127) fires, release (0) does not.
    assert_eq!(
        rig.on_input("FS-6", &[0xB0, 80, 127], 1),
        vec![RigOut::Action {
            action: Action::Next
        }]
    );
    assert!(rig.on_input("FS-6", &[0xB0, 80, 0], 2).is_empty());
    // Clock and SysEx never trigger.
    assert!(
        rig.on_input("FS-6", &[0xF8, 0xF0, 0x01, 0xF7], 3)
            .is_empty()
    );
}

#[test]
fn config_round_trips_as_toml() {
    let mut cfg = RigConfig::demo();
    cfg.mappings.push(Mapping {
        trigger: Trigger::ProgramChange {
            channel: Some(1),
            program: 5,
        },
        action: Action::NextSong,
        port: Some("FS".into()),
    });
    let text = toml::to_string(&cfg).expect("toml");
    let back: RigConfig = toml::from_str(&text).expect("parse");
    assert_eq!(back, cfg);
    let json = serde_json::to_string(&cfg).expect("json");
    let back: RigConfig = serde_json::from_str(&json).expect("parse");
    assert_eq!(back, cfg);
}
