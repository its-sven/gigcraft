//! Offline rendering tests: the engine is driven exactly as a host would,
//! with generated audio.

use crate::program::{ACCENT_CLICK, BEAT_CLICK, click_sound};
use crate::*;
use craft_audio_dsp::pan::{PanLaw, gains};

const SR: u32 = 48_000;

fn eng() -> (Controller, Renderer) {
    engine(EngineConfig::new(SR))
}

/// Render `frames` frames in 512-frame host buffers.
fn run(r: &mut Renderer, frames: usize, channels: usize) -> Vec<f32> {
    let mut all = vec![0.0; frames * channels];
    for chunk in all.chunks_mut(512 * channels) {
        r.render(chunk, channels);
    }
    all
}

fn channel(out: &[f32], channels: usize, c: usize) -> Vec<f32> {
    out.chunks_exact(channels).map(|f| f[c]).collect()
}

fn stereo(l: Vec<f32>, r: Vec<f32>) -> Clip {
    Clip::from_planar(SR, vec![l, r], SR).unwrap()
}

fn dc(level: f32, frames: usize) -> Clip {
    stereo(vec![level; frames], vec![level; frames])
}

/// Each sample holds its own frame number, so positions can be read back exactly.
fn counter(frames: usize) -> Clip {
    let v: Vec<f32> = (0..frames).map(|i| i as f32).collect();
    stereo(v.clone(), v)
}

fn song(tracks: Vec<TrackSetup>) -> SongSetup {
    SongSetup {
        tracks,
        ..SongSetup::default()
    }
}

fn tempo(bpm: f64, beats_per_bar: u32) -> Option<Tempo> {
    Some(Tempo { bpm, beats_per_bar })
}

fn max_step(v: &[f32]) -> f32 {
    v.windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn silence_when_stopped_or_empty() {
    let (mut c, mut r) = eng();
    let out = run(&mut r, 2048, 2);
    assert!(out.iter().all(|s| *s == 0.0));
    c.load(song(vec![TrackSetup::new(dc(0.5, 48_000))]))
        .unwrap();
    let mut out = vec![1.0; 4096];
    r.render(&mut out, 2);
    assert!(out.iter().all(|s| *s == 0.0), "stopped renders silence");
    assert_eq!(c.status().state, PlayState::Stopped);
    // Zero channels and odd lengths do not panic.
    r.render(&mut out, 0);
    let mut odd = vec![1.0; 5];
    r.render(&mut odd, 2);
    assert!(odd.iter().all(|s| *s == 0.0));
}

#[test]
fn gain_and_pan_math() {
    let (mut c, mut r) = eng();
    let mono = Clip::from_planar(SR, vec![vec![0.5; 4800]], SR).unwrap();
    let mut centre = TrackSetup::new(mono.clone());
    centre.gain_db = -6.0;
    let mut left = TrackSetup::new(mono);
    left.pan = -1.0;
    left.bus = 1;
    let mut balance = TrackSetup::new(stereo(vec![0.4; 4800], vec![0.8; 4800]));
    balance.pan = 0.5;
    balance.bus = 2;
    c.load(song(vec![centre, left, balance])).unwrap();
    c.play(false).unwrap();
    let out = run(&mut r, 1000, 6);
    let f = &out[600..606];
    let g6 = 10f32.powf(-6.0 / 20.0);
    let (cl, cr) = gains(0.0, PanLaw::Minus3);
    assert!(
        (cl - std::f32::consts::FRAC_1_SQRT_2).abs() < 2e-3,
        "−3 dB centre"
    );
    assert_eq!(f[0], 0.5 * cl * g6);
    assert_eq!(f[1], 0.5 * cr * g6);
    assert!((f[0] - 0.5 * 0.501_187 * std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    assert_eq!((f[2], f[3]), (0.5, 0.0), "hard left");
    assert_eq!(
        (f[4], f[5]),
        (0.2, 0.8),
        "stereo balance halves the left side"
    );
}

#[test]
fn click_is_sample_accurate_at_120_bpm() {
    let (mut c, mut r) = eng();
    c.load(SongSetup {
        tempo: tempo(120.0, 4),
        click: ClickSetup {
            enabled: true,
            ..ClickSetup::default()
        },
        ..SongSetup::default()
    })
    .unwrap();
    c.play(false).unwrap();
    let out = channel(&run(&mut r, 4 * SR as usize, 2), 2, 0);
    let accent = click_sound(ACCENT_CLICK, SR);
    let beat = click_sound(BEAT_CLICK, SR);
    for (f, &got) in out.iter().enumerate() {
        let (k, off) = (f / 24_000, f % 24_000);
        let sound = if k % 4 == 0 { &accent } else { &beat };
        let want = sound.get(off).copied().unwrap_or(0.0);
        assert_eq!(got, want, "frame {f}");
    }
    // A click starts at 0 and ends at 0.
    assert_eq!(accent[0], 0.0);
    assert_eq!(*accent.last().unwrap(), 0.0);
    assert!(max_step(&accent) < 0.2);
    // Each beat starts on its frame: silent on the beat frame, sounding right after.
    for k in 1..8 {
        let at = k * 24_000;
        assert_eq!(out[at - 1], 0.0);
        assert_ne!(out[at + 1], 0.0);
    }
    assert_eq!(
        c.status().state,
        PlayState::Playing,
        "a click alone runs on"
    );
}

#[test]
fn click_follows_an_odd_tempo_without_drift() {
    let (mut c, mut r) = eng();
    c.load(SongSetup {
        tempo: tempo(97.0, 3),
        click: ClickSetup {
            enabled: true,
            ..ClickSetup::default()
        },
        ..SongSetup::default()
    })
    .unwrap();
    c.play(false).unwrap();
    let out = channel(&run(&mut r, 60 * SR as usize, 1), 1, 0);
    let beat = f64::from(SR) * 60.0 / 97.0;
    for k in 1..97 {
        let at = (k as f64 * beat).round() as usize;
        assert_eq!(out[at], 0.0, "beat {k}");
        assert_eq!(out[at - 1], 0.0, "silence before beat {k}");
        assert_ne!(out[at + 1], 0.0, "beat {k} sounds");
    }
}

#[test]
fn count_in_then_tracks_then_end() {
    let (mut c, mut r) = eng();
    let handle = c
        .load(SongSetup {
            tracks: vec![TrackSetup::new(dc(0.25, SR as usize))],
            tempo: tempo(120.0, 4),
            click: ClickSetup {
                enabled: false,
                count_in_bars: 2,
                bus: 1,
                ..ClickSetup::default()
            },
            markers: Vec::new(),
        })
        .unwrap();
    c.play(true).unwrap();
    let first = run(&mut r, 512, 4);
    let status = c.status();
    assert!(status.counting_in);
    assert_eq!(status.position_frames, 512 - 192_000);
    let rest = run(&mut r, 192_000 + 48_000 + 4096 - 512, 4);
    let out = [first, rest].concat();
    let track = channel(&out, 4, 0);
    let click = channel(&out, 4, 2);
    let count_in = 2 * 4 * 24_000;
    assert!(track[..count_in].iter().all(|s| *s == 0.0));
    assert!(
        track[count_in..count_in + 48_000]
            .iter()
            .all(|s| *s == 0.25)
    );
    assert!(track[count_in + 48_000..].iter().all(|s| *s == 0.0));
    // Eight clicks, accents on both bar lines, nothing once the song runs.
    let onsets: Vec<usize> = (1..click.len())
        .filter(|&f| click[f] != 0.0 && click[f - 1] == 0.0)
        .map(|f| f - 1)
        .collect();
    assert_eq!(onsets, (0..8).map(|k| k * 24_000).collect::<Vec<_>>());
    let peak = |at: usize| {
        click[at..at + 1440]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()))
    };
    assert!(peak(0) > peak(24_000) && peak(96_000) > peak(120_000));
    assert_eq!(
        c.events(),
        [Event::CountInFinished(handle), Event::Ended(handle)]
    );
    let status = c.status();
    assert_eq!(status.state, PlayState::Stopped);
    assert_eq!(status.position_frames, 0, "rewound after the end");
}

#[test]
fn fades_reach_zero_without_steps() {
    let (mut c, mut r) = eng();
    let handle = c
        .load(song(vec![TrackSetup::new(dc(1.0, 10 * SR as usize))]))
        .unwrap();
    c.fade_in(100.0).unwrap();
    let fade_in = channel(&run(&mut r, 6000, 2), 2, 0);
    assert_eq!(fade_in[0], 0.0);
    assert!(max_step(&fade_in) <= 1.0 / 4800.0 + 1e-6);
    assert!(fade_in.windows(2).all(|w| w[1] >= w[0]));
    assert_eq!(fade_in[5999], 1.0);

    c.fade_out(100.0).unwrap();
    let fade_out = channel(&run(&mut r, 6000, 2), 2, 0);
    assert_eq!(fade_out[0], 1.0);
    assert!(max_step(&fade_out) <= 1.0 / 4800.0 + 1e-6);
    assert!(fade_out.windows(2).all(|w| w[1] <= w[0]));
    let last = fade_out.iter().rposition(|s| *s != 0.0).unwrap();
    assert!((4795..=4801).contains(&last), "silent after 100 ms: {last}");
    assert!(
        fade_out[last] <= 1.0 / 4800.0 + 1e-6,
        "the last step is tiny"
    );
    assert_eq!(c.status().state, PlayState::Stopped);
    assert_eq!(c.events(), [Event::FadedOut(handle)]);
}

#[test]
fn stop_and_pause_fade_briefly_and_pause_keeps_the_place() {
    let (mut c, mut r) = eng();
    c.load(song(vec![TrackSetup::new(counter(SR as usize))]))
        .unwrap();
    c.play(false).unwrap();
    run(&mut r, 10_240, 2);
    c.pause().unwrap();
    let out = channel(&run(&mut r, 1024, 2), 2, 0);
    let declick = 240;
    assert_eq!(out[0], 10_240.0);
    assert!(out[declick..].iter().all(|s| *s == 0.0));
    let gains: Vec<f32> = (0..declick).map(|i| out[i] / (10_240 + i) as f32).collect();
    assert!(max_step(&gains) <= 1.0 / 240.0 + 1e-5);
    let status = c.status();
    assert_eq!(status.state, PlayState::Paused);
    assert_eq!(status.position_frames, 10_240 + declick as i64);

    c.play(false).unwrap();
    let out = channel(&run(&mut r, 1024, 2), 2, 0);
    assert_eq!(out[0], 0.0, "resumes from silence");
    assert_eq!(out[500], (10_480 + 500) as f32);

    c.stop().unwrap();
    let out = channel(&run(&mut r, 1024, 2), 2, 0);
    assert!(out[declick..].iter().all(|s| *s == 0.0));
    assert!(out[declick - 1] < 100.0, "ramped down, not cut");
    assert_eq!(c.status().position_frames, 0);
    assert_eq!(c.status().state, PlayState::Stopped);
}

#[test]
fn loop_region_wraps_exactly() {
    let (mut c, mut r) = eng();
    c.load(SongSetup {
        tracks: vec![TrackSetup::new(counter(2 * SR as usize))],
        markers: vec![MarkerSetup {
            name: "Vamp".into(),
            start_secs: 0.5,
            end_secs: Some(1.0),
        }],
        ..SongSetup::default()
    })
    .unwrap();
    c.loop_region(Some("Vamp")).unwrap();
    c.play(false).unwrap();
    let out = channel(&run(&mut r, 96_000, 2), 2, 0);
    for (f, &got) in out.iter().enumerate() {
        let want = if f < 48_000 {
            f
        } else {
            24_000 + (f - 48_000) % 24_000
        };
        assert_eq!(got, want as f32, "frame {f}");
    }
    assert!(c.status().looping);
    // The last frame played was the region's last; without the loop the
    // song runs on from there.
    c.loop_region(None).unwrap();
    let out = channel(&run(&mut r, 30_000, 2), 2, 0);
    assert_eq!(out[0], 48_000.0);
    assert_eq!(
        out[29_999],
        (48_000 + 29_999) as f32,
        "plays on past the region"
    );
    assert!(!c.status().looping);
}

#[test]
fn jump_on_the_next_bar_is_sample_exact() {
    let (mut c, mut r) = eng();
    c.load(SongSetup {
        tracks: vec![TrackSetup::new(counter(10 * SR as usize))],
        tempo: tempo(120.0, 4),
        markers: vec![MarkerSetup {
            name: "Chorus".into(),
            start_secs: 5.0,
            end_secs: None,
        }],
        ..SongSetup::default()
    })
    .unwrap();
    c.play(false).unwrap();
    let a = run(&mut r, 30_208, 2);
    c.jump_to_marker("Chorus", JumpAt::NextBar).unwrap();
    let b = run(&mut r, 100_000, 2);
    let out = channel(&[a, b].concat(), 2, 0);
    for (f, &got) in out.iter().enumerate() {
        let want = if f < 96_000 { f } else { 240_000 + f - 96_000 };
        assert_eq!(got, want as f32, "frame {f}");
    }
}

#[test]
fn jump_now_dips_briefly() {
    let (mut c, mut r) = eng();
    c.load(SongSetup {
        tracks: vec![TrackSetup::new(counter(10 * SR as usize))],
        markers: vec![MarkerSetup {
            name: "Bridge".into(),
            start_secs: 5.0,
            end_secs: None,
        }],
        ..SongSetup::default()
    })
    .unwrap();
    c.play(false).unwrap();
    run(&mut r, 1024, 2);
    c.jump_to_marker("Bridge", JumpAt::Now).unwrap();
    let out = channel(&run(&mut r, 1024, 2), 2, 0);
    assert_eq!(out[0], 1024.0);
    assert_eq!(out[240], 0.0, "silent at the cut");
    assert_eq!(out[600], (240_000 + 360) as f32, "running from the marker");
}

#[test]
fn crossfade_hands_over_to_the_next_song() {
    let (mut c, mut r) = eng();
    let n = 10 * SR as usize;
    let a = c
        .load(song(vec![TrackSetup::new(stereo(
            vec![1.0; n],
            vec![0.0; n],
        ))]))
        .unwrap();
    c.play(false).unwrap();
    run(&mut r, 1024, 2);
    let b = c
        .crossfade_to(
            song(vec![TrackSetup::new(stereo(vec![0.0; n], vec![1.0; n]))]),
            100.0,
        )
        .unwrap();
    let out = run(&mut r, 6000, 2);
    let (l, rr) = (channel(&out, 2, 0), channel(&out, 2, 1));
    assert_eq!((l[0], rr[0]), (1.0, 0.0));
    for (x, y) in l.iter().zip(&rr) {
        assert!((x + y - 1.0).abs() < 1e-4, "linear crossfade keeps the sum");
    }
    assert!(max_step(&l) <= 1.0 / 4800.0 + 1e-6);
    assert_eq!((l[5999], rr[5999]), (0.0, 1.0));
    assert_eq!(c.events(), [Event::FadedOut(a)]);
    let status = c.status();
    assert_eq!(status.song, Some(b));
    assert_eq!(status.state, PlayState::Playing);
    assert_eq!(status.position_frames, 6000);
}

#[test]
fn load_while_playing_stops_the_old_song_softly() {
    let (mut c, mut r) = eng();
    c.load(song(vec![TrackSetup::new(dc(1.0, SR as usize))]))
        .unwrap();
    c.play(false).unwrap();
    run(&mut r, 512, 2);
    let next = c
        .load(song(vec![TrackSetup::new(dc(0.5, SR as usize))]))
        .unwrap();
    let out = channel(&run(&mut r, 1024, 2), 2, 0);
    assert!(max_step(&out) <= 1.0 / 240.0 + 1e-5);
    assert!(out[300..].iter().all(|s| *s == 0.0));
    assert_eq!(c.status().song, Some(next));
    assert_eq!(c.status().state, PlayState::Stopped);
    c.play(false).unwrap();
    assert_eq!(channel(&run(&mut r, 512, 2), 2, 0)[0], 0.5);
}

#[test]
fn buses_route_to_output_pairs() {
    let setup = || SongSetup {
        tracks: vec![
            TrackSetup::new(dc(0.1, SR as usize)),
            TrackSetup {
                bus: 2,
                ..TrackSetup::new(dc(0.2, SR as usize))
            },
        ],
        tempo: tempo(120.0, 4),
        click: ClickSetup {
            enabled: true,
            bus: 1,
            ..ClickSetup::default()
        },
        markers: Vec::new(),
    };
    let (mut c, mut r) = eng();
    c.load(setup()).unwrap();
    c.play(false).unwrap();
    let out = run(&mut r, 2048, 6);
    for f in out.as_chunks::<6>().0 {
        assert_eq!((f[0], f[1], f[4], f[5]), (0.1, 0.1, 0.2, 0.2));
    }
    assert!(
        channel(&out, 6, 2).iter().any(|s| *s != 0.0),
        "click on 3/4"
    );
    assert_eq!(channel(&out, 6, 2), channel(&out, 6, 3));
    assert!(!c.status().unrouted);

    // A stereo device: bus 2 and the click have nowhere to go.
    let (mut c, mut r) = eng();
    c.load(setup()).unwrap();
    c.play(false).unwrap();
    let out = run(&mut r, 2048, 2);
    assert!(out.iter().all(|s| *s == 0.1));
    assert!(c.status().unrouted);

    // A mono device: bus 0 folds to one channel.
    let (mut c, mut r) = eng();
    c.load(song(vec![TrackSetup {
        pan: -1.0,
        ..TrackSetup::new(Clip::from_planar(SR, vec![vec![0.5; 4800]], SR).unwrap())
    }]))
    .unwrap();
    c.play(false).unwrap();
    assert!(run(&mut r, 1024, 1).iter().all(|s| *s == 0.25));

    // Moving a track to another bus while playing.
    let (mut c, mut r) = eng();
    c.load(setup()).unwrap();
    c.play(false).unwrap();
    c.set_track_bus(0, 2).unwrap();
    let out = run(&mut r, 512, 6);
    assert!((out[4] - 0.3).abs() < 1e-6 && out[0] == 0.0);
}

#[test]
fn mute_solo_and_gain_glide() {
    let (mut c, mut r) = eng();
    c.load(song(vec![
        TrackSetup::new(dc(0.1, SR as usize)),
        TrackSetup {
            bus: 1,
            ..TrackSetup::new(dc(0.2, SR as usize))
        },
    ]))
    .unwrap();
    c.play(false).unwrap();
    run(&mut r, 512, 4);
    c.set_track_solo(1, true).unwrap();
    let out = run(&mut r, 1024, 4);
    let a = channel(&out, 4, 0);
    assert_eq!(a[0], 0.1);
    assert!(max_step(&a) <= 0.1 / 480.0 + 1e-6, "glides over 10 ms");
    assert!(a[480..].iter().all(|s| *s == 0.0));
    assert!(channel(&out, 4, 2).iter().all(|s| *s == 0.2));
    c.set_track_solo(1, false).unwrap();
    c.set_track_mute(1, true).unwrap();
    c.set_track_gain_db(0, 6.0).unwrap();
    let out = run(&mut r, 1024, 4);
    assert!((channel(&out, 4, 0)[1000] - 0.1 * 10f32.powf(0.3)).abs() < 1e-6);
    assert_eq!(channel(&out, 4, 2)[1000], 0.0);
    // Bad numbers never reach the output.
    c.set_track_gain_db(0, f32::NAN).unwrap();
    c.set_track_pan(0, f32::INFINITY).unwrap();
    c.set_click_gain_db(f32::NAN).unwrap();
    assert!(run(&mut r, 1024, 4).iter().all(|s| s.is_finite()));
}

#[test]
fn block_size_does_not_change_the_output() {
    let setup = || SongSetup {
        tracks: vec![
            TrackSetup::new(counter(3 * SR as usize)),
            TrackSetup {
                pan: 0.3,
                gain_db: -2.0,
                ..TrackSetup::new(Clip::from_planar(SR, vec![vec![0.3; 50_000]], SR).unwrap())
            },
        ],
        tempo: tempo(133.0, 7),
        click: ClickSetup {
            enabled: true,
            count_in_bars: 1,
            bus: 1,
            gain_db: -3.0,
            accent: true,
        },
        markers: vec![MarkerSetup {
            name: "Loop".into(),
            start_secs: 0.25,
            end_secs: Some(0.75),
        }],
    };
    let render = |sizes: &[usize], max_block: usize| {
        let (mut c, mut r) = engine(EngineConfig {
            max_block_frames: max_block,
            ..EngineConfig::new(SR)
        });
        c.load(setup()).unwrap();
        c.loop_region(Some("Loop")).unwrap();
        c.play(true).unwrap();
        let mut out = vec![0.0; 150_000 * 4];
        let mut at = 0;
        let mut i = 0;
        while at < 150_000 {
            let n = sizes[i % sizes.len()].min(150_000 - at);
            r.render(&mut out[at * 4..(at + n) * 4], 4);
            at += n;
            i += 1;
        }
        out
    };
    let whole = render(&[150_000], 4096);
    assert!(whole.iter().any(|s| *s != 0.0));
    assert_eq!(render(&[1, 7, 64, 333, 1024], 4096), whole);
    assert_eq!(render(&[150_000], 16), whole);
}

#[test]
fn meters_report_peaks_per_channel() {
    let (mut c, mut r) = eng();
    c.load(song(vec![TrackSetup::new(stereo(
        vec![0.5; 4800],
        vec![-0.25; 4800],
    ))]))
    .unwrap();
    c.play(false).unwrap();
    run(&mut r, 1024, 2);
    let m = c.take_meters();
    assert_eq!((m[0], m[1], m[2]), (0.5, 0.25, 0.0));
    assert_eq!(c.take_meters()[0], 0.0, "reset after reading");
}

#[test]
fn controller_errors_are_values() {
    let (mut c, mut r) = engine(EngineConfig {
        queue_capacity: 2,
        ..EngineConfig::new(SR)
    });
    assert_eq!(c.play(false), Err(EngineError::NoSong));
    c.load(SongSetup {
        tracks: vec![TrackSetup::new(dc(0.1, 100))],
        markers: vec![MarkerSetup {
            name: "Point".into(),
            start_secs: 0.0,
            end_secs: None,
        }],
        ..SongSetup::default()
    })
    .unwrap();
    assert_eq!(c.markers(), ["Point"]);
    assert_eq!(
        c.jump_to_marker("Nope", JumpAt::Now),
        Err(EngineError::UnknownMarker("Nope".into()))
    );
    assert_eq!(
        c.loop_region(Some("Point")),
        Err(EngineError::NotARegion("Point".into()))
    );
    assert_eq!(c.set_track_mute(3, true), Err(EngineError::NoTrack(3)));
    c.play(false).unwrap();
    assert_eq!(c.play(false), Err(EngineError::QueueFull));
    run(&mut r, 64, 2);
    c.play(false).unwrap();
    drop(r);
    run_nothing(&mut c);
}

fn run_nothing(c: &mut Controller) {
    assert_eq!(c.stop(), Err(EngineError::Disconnected));
}

fn sine(freq: f32, frames: usize, rate: u32) -> Vec<f32> {
    (0..frames)
        .map(|i| 0.5 * (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin())
        .collect()
}

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt()
}

#[test]
fn decodes_every_format_and_resamples() {
    use craft_audio::{AudioBuffer, EncodeFormat, EncodeOptions, encode};
    let tone = sine(440.0, 44_100, 44_100);
    let buf = AudioBuffer {
        sample_rate: 44_100,
        channels: vec![tone.clone(), tone],
    };
    for (format, ext) in [
        (EncodeFormat::Wav, "wav"),
        (EncodeFormat::Aiff, "aif"),
        (EncodeFormat::Flac, "flac"),
        (EncodeFormat::Mp3, "mp3"),
        (EncodeFormat::Aac, "m4a"),
    ] {
        let bytes = encode(&buf, &EncodeOptions::new(format)).unwrap();
        let clip = Clip::decode(&bytes, Some(ext), SR).unwrap_or_else(|e| panic!("{ext}: {e}"));
        assert_eq!(clip.sample_rate(), SR);
        assert_eq!(clip.channels(), 2);
        assert!(
            clip.frames().abs_diff(48_000) < 2_400,
            "{ext}: {} frames",
            clip.frames()
        );
        let middle = &clip.samples(0)[10_000..30_000];
        assert!(
            (rms(middle) - 0.3536).abs() < 0.03,
            "{ext}: rms {}",
            rms(middle)
        );
        assert!(clip.samples(1).iter().all(|s| s.is_finite()));
    }
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert!(Clip::decode(b"not audio at all", Some("wav"), SR).is_err());
    assert!(Clip::decode(&[], None, SR).is_err());
    // Every truncation of a valid file decodes or fails; none panics.
    let wav = craft_audio::pcm_wav_file(&[7u8; 400], 2, 44_100, 16);
    for len in 0..wav.len() {
        let _ = Clip::decode(&wav[..len], Some("wav"), SR);
    }
    assert!(Clip::decode(&wav[..20], Some("wav"), SR).is_err());
    assert!(matches!(
        Clip::from_planar(SR, vec![], SR),
        Err(LoadError::Empty)
    ));
    assert!(matches!(
        Clip::from_planar(0, vec![vec![0.0; 10]], SR),
        Err(LoadError::SampleRate(0))
    ));
    assert!(matches!(
        Clip::from_planar(SR, vec![vec![0.0; 10]], 5),
        Err(LoadError::SampleRate(5))
    ));
    let nan = Clip::from_planar(
        SR,
        vec![vec![f32::NAN; 10], vec![1.0; 12], vec![2.0; 10]],
        SR,
    )
    .unwrap();
    assert_eq!((nan.channels(), nan.frames()), (2, 10));
    assert_eq!(nan.samples(0)[0], 0.0, "non-finite samples become silence");
}

#[test]
fn setup_from_a_library_song() {
    use gigcraft_model::{ClickSettings, Marker, Song, Track};
    let mut s = Song::new("Six Eight");
    s.tempo_bpm = Some(90.0);
    s.time_signature = Some("6/8".into());
    let mut guide = Track::new("guide.wav");
    guide.bus = 1;
    guide.gain_db = -4.0;
    guide.mute = true;
    s.tracks = vec![Track::new("band.wav"), guide];
    s.click = Some(ClickSettings {
        enabled: true,
        bus: 1,
        count_in_bars: 2,
        volume_db: -6.0,
    });
    s.markers = vec![Marker {
        name: "Outro".into(),
        at_secs: 100.0,
        end_secs: Some(110.0),
    }];
    let mut asked = Vec::new();
    let setup = SongSetup::from_song(&s, |t| {
        asked.push(t.file.clone());
        Ok(dc(0.1, 100))
    })
    .unwrap();
    assert_eq!(asked, ["band.wav", "guide.wav"]);
    assert_eq!(setup.tempo, tempo(90.0, 6));
    assert_eq!(setup.click.count_in_bars, 2);
    assert_eq!(setup.click.bus, 1);
    assert!((setup.tracks[1].gain_db + 4.0).abs() < 1e-6 && setup.tracks[1].mute);
    assert_eq!(setup.markers[0].end_secs, Some(110.0));

    let err = SongSetup::from_song(&s, |_| Err(LoadError::Empty)).unwrap_err();
    assert!(matches!(err, LoadError::Track { ref file, .. } if file == "band.wav"));
    assert!(err.to_string().contains("band.wav"));
}
