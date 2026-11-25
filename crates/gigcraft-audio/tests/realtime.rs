//! `Renderer::render` must be safe on a real-time thread: this test counts
//! every allocation and free made on the rendering thread while it runs
//! through loads, crossfades, count-ins, loops, jumps and song ends.

#![allow(
    unsafe_code,
    reason = "a counting global allocator is the only way to see allocations; test only"
)]
#![allow(clippy::unwrap_used, reason = "test")]

use gigcraft_audio::{
    ClickSetup, Clip, EngineConfig, JumpAt, MarkerSetup, SongSetup, Tempo, TrackSetup, engine,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static COUNT: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

fn note() {
    if ARMED.with(Cell::get) {
        COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

// SAFETY: every call is forwarded unchanged to the system allocator.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SR: u32 = 48_000;

fn song(level: f32) -> SongSetup {
    let n = SR as usize * 4;
    let clip = Clip::from_planar(SR, vec![vec![level; n], vec![level; n]], SR).unwrap();
    let mono = Clip::from_planar(SR, vec![vec![level; n / 2]], SR).unwrap();
    SongSetup {
        tracks: vec![
            TrackSetup::new(clip),
            TrackSetup {
                bus: 1,
                pan: -0.5,
                ..TrackSetup::new(mono)
            },
        ],
        tempo: Some(Tempo {
            bpm: 140.0,
            beats_per_bar: 4,
        }),
        click: ClickSetup {
            enabled: true,
            count_in_bars: 1,
            bus: 1,
            ..ClickSetup::default()
        },
        markers: vec![MarkerSetup {
            name: "A".into(),
            start_secs: 0.5,
            end_secs: Some(1.0),
        }],
    }
}

#[test]
fn render_never_allocates_or_frees() {
    let (mut control, mut renderer) = engine(EngineConfig {
        max_block_frames: 256,
        ..EngineConfig::new(SR)
    });
    let mut out = vec![0.0f32; 1024 * 4];
    let mut render = |blocks: usize| {
        ARMED.with(|a| a.set(true));
        for _ in 0..blocks {
            renderer.render(&mut out, 4);
        }
        ARMED.with(|a| a.set(false));
    };

    control.load(song(0.1)).unwrap();
    control.play(true).unwrap();
    render(60); // count-in into the tracks
    control.loop_region(Some("A")).unwrap();
    control.set_track_gain_db(0, -3.0).unwrap();
    control.set_track_solo(1, true).unwrap();
    render(60);
    control.jump_to_marker("A", JumpAt::NextBar).unwrap();
    control.loop_region(None).unwrap();
    render(10);
    control.jump_to_marker("A", JumpAt::Now).unwrap();
    render(10);
    control.crossfade_to(song(0.2), 300.0).unwrap();
    render(5);
    // A second crossfade while the first is still running cuts the oldest song.
    control.crossfade_to(song(0.3), 300.0).unwrap();
    render(30);
    // Loading while playing retires the playing song.
    control.load(song(0.4)).unwrap();
    render(5);
    control.play(false).unwrap();
    render(250); // to the end of the song
    control.fade_in(50.0).unwrap();
    render(5);
    control.pause().unwrap();
    render(5);
    control.fade_out(20.0).unwrap();
    control.stop().unwrap();
    render(5);

    assert_eq!(
        COUNT.load(Ordering::Relaxed),
        0,
        "render allocated or freed"
    );
    assert!(!control.events().is_empty());
    control.collect();

    // The counter does see allocations on this thread.
    ARMED.with(|a| a.set(true));
    drop(std::hint::black_box(vec![0u8; 64]));
    ARMED.with(|a| a.set(false));
    assert_eq!(COUNT.load(Ordering::Relaxed), 2);
}
