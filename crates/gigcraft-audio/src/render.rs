//! The audio side: renders songs into the host's output buffer.
//!
//! [`Renderer::render`] runs on the real-time thread. It takes no lock, makes
//! no allocation and frees nothing: songs arrive boxed through the command
//! queue and leave through the `retired` channel (see `control.rs`). All
//! scratch memory is allocated in `Renderer::new`.
//!
//! Each block is rendered in two passes per deck. The timeline pass walks the
//! deck frame by frame (count-in, loops, jumps, fades, the end of the song)
//! and notes where every output frame reads the tracks and the click and at
//! which gain. The mix pass then adds every track and the click into the
//! interleaved output. Two decks let one song fade out while the next fades in.

use crate::EngineConfig;
use crate::control::{
    COUNTING_IN, ClickParam, Command, Event, JumpAt, LOOPING, STATE_MASK, Shared, SongHandle,
    TrackParam, UNROUTED,
};
use crate::program::{Program, Ramp, db_to_gain, ms_to_frames};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, SyncSender};

/// Stop, pause, seek and immediate jumps fade over this long, so they never click.
pub const DECLICK_MS: f32 = 5.0;

/// An output frame that reads nothing.
const SILENT: i64 = i64::MIN;

/// What happens when a ramp has reached 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Then {
    Nothing,
    Stop,
    Pause,
    Seek(i64),
    Retire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeckState {
    Stopped,
    Playing,
    Paused,
}

/// One song slot with its transport.
#[derive(Debug)]
struct Deck {
    program: Option<Box<Program>>,
    state: DeckState,
    /// Next frame of the tracks to play; negative during the count-in.
    pos: i64,
    /// While counting in: the frame the tracks start at.
    count_in_to: Option<i64>,
    /// User fades and crossfades.
    fade: Ramp,
    fade_then: Then,
    /// Short fades around stop, pause, seek and jumps.
    declick: Ramp,
    declick_then: Then,
    loop_region: Option<(i64, i64)>,
    /// A jump waiting for a bar line: (at, to).
    bar_jump: Option<(i64, i64)>,
    /// The song is done here; hand it back after this block.
    retire: bool,
}

impl Deck {
    fn new(program: Option<Box<Program>>) -> Self {
        Self {
            program,
            state: DeckState::Stopped,
            pos: 0,
            count_in_to: None,
            fade: Ramp::at(1.0),
            fade_then: Then::Nothing,
            declick: Ramp::at(1.0),
            declick_then: Then::Nothing,
            loop_region: None,
            bar_jump: None,
            retire: false,
        }
    }

    fn id(&self) -> u64 {
        self.program.as_ref().map_or(0, |p| p.id)
    }

    fn halt(&mut self, state: DeckState) {
        self.state = state;
        self.count_in_to = None;
        self.bar_jump = None;
        self.fade = Ramp::at(1.0);
        self.declick = Ramp::at(1.0);
        self.fade_then = Then::Nothing;
        self.declick_then = Then::Nothing;
    }

    /// Run `then`. `faded` says a user fade (not a declick) has just ended.
    fn apply(&mut self, then: Then, faded: bool, ctx: &Ctx<'_>) {
        let id = self.id();
        match then {
            Then::Nothing => return,
            Then::Stop => {
                self.halt(DeckState::Stopped);
                self.pos = 0;
                self.retire = !ctx.active;
            }
            Then::Pause => {
                if let Some(to) = self.count_in_to {
                    self.pos = to;
                }
                self.halt(DeckState::Paused);
                self.retire = !ctx.active;
            }
            Then::Seek(to) => {
                self.pos = to;
                self.count_in_to = None;
                self.bar_jump = None;
                self.declick.to(1.0, ctx.declick);
                self.declick_then = Then::Nothing;
            }
            Then::Retire => {
                self.halt(DeckState::Stopped);
                self.retire = true;
            }
        }
        if faded && matches!(then, Then::Stop | Then::Retire) {
            let _ = ctx.events.try_send(Event::FadedOut(SongHandle(id)));
        }
    }
}

/// What the timeline pass needs besides the deck.
struct Ctx<'a> {
    active: bool,
    declick: u32,
    events: &'a SyncSender<Event>,
}

/// Where a bus lands in the output.
#[derive(Debug, Clone, Copy)]
enum Route {
    Stereo(usize, usize),
    /// Only the left channel of the pair exists: L+R summed into it.
    Mono(usize),
    None,
}

impl Route {
    fn new(bus: usize, channels: usize) -> Self {
        let left = bus.saturating_mul(2);
        let right = left.saturating_add(1);
        if right < channels {
            Self::Stereo(left, right)
        } else if left < channels {
            Self::Mono(left)
        } else {
            Self::None
        }
    }

    #[inline]
    fn add(self, out: &mut [f32], channels: usize, frame: usize, l: f32, r: f32) {
        let base = frame * channels;
        match self {
            Self::Stereo(cl, cr) => {
                if let Some(s) = out.get_mut(base + cl) {
                    *s += l;
                }
                if let Some(s) = out.get_mut(base + cr) {
                    *s += r;
                }
            }
            Self::Mono(c) => {
                if let Some(s) = out.get_mut(base + c) {
                    *s += 0.5 * (l + r);
                }
            }
            Self::None => {}
        }
    }
}

/// The real-time half of the engine. Move it into the audio callback.
#[derive(Debug)]
pub struct Renderer {
    rate: u32,
    max_block: usize,
    declick: u32,
    commands: Receiver<Command>,
    retired: SyncSender<Box<Program>>,
    events: SyncSender<Event>,
    shared: Arc<Shared>,
    decks: [Deck; 2],
    active: usize,
    track_pos: Vec<i64>,
    click_pos: Vec<i64>,
    gain: Vec<f32>,
    unrouted: bool,
}

impl Renderer {
    pub(crate) fn new(
        config: EngineConfig,
        commands: Receiver<Command>,
        retired: SyncSender<Box<Program>>,
        events: SyncSender<Event>,
        shared: Arc<Shared>,
    ) -> Self {
        let max_block = config.max_block_frames.clamp(16, 1 << 16);
        Self {
            rate: config.sample_rate,
            max_block,
            declick: ms_to_frames(DECLICK_MS, config.sample_rate),
            commands,
            retired,
            events,
            shared,
            decks: [Deck::new(None), Deck::new(None)],
            active: 0,
            track_pos: vec![SILENT; max_block],
            click_pos: vec![SILENT; max_block],
            gain: vec![0.0; max_block],
            unrouted: false,
        }
    }

    /// The sample rate this renderer was made for.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    /// Fill `out` (interleaved, `channels` per frame) with the next frames.
    /// Bus *n* plays on channels 2n and 2n+1 (counting from 0); a bus whose
    /// pair is missing is dropped and flagged in the status. Never allocates,
    /// locks or blocks.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        out.fill(0.0);
        self.apply_commands();
        if channels == 0 {
            return;
        }
        self.unrouted = false;
        let frames = out.len() / channels;
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(self.max_block);
            let Some(block) = out.get_mut(done * channels..(done + n) * channels) else {
                break;
            };
            for d in 0..2 {
                self.render_deck(d, block, channels, n);
            }
            done += n;
        }
        self.publish(out, channels);
    }

    fn apply_commands(&mut self) {
        while let Ok(cmd) = self.commands.try_recv() {
            self.apply_command(cmd);
        }
    }

    fn retire(&self, program: Box<Program>) {
        // The controller drains this channel before every command it sends,
        // and it holds more than the queue can produce, so this does not fail
        // in practice. If it does (the controller is gone), the song is
        // dropped here.
        let _ = self.retired.try_send(program);
    }

    fn swap_in(&mut self, program: Box<Program>, crossfade: Option<u32>) {
        let incoming = 1 - self.active;
        // A song still fading out of the other deck from an earlier crossfade
        // is cut now.
        if let Some(old) = self.decks[incoming].program.take() {
            self.retire(old);
        }
        self.decks[incoming] = Deck::new(Some(program));
        let declick = self.declick;
        let outgoing = &mut self.decks[self.active];
        if outgoing.program.is_some() {
            if outgoing.state == DeckState::Playing {
                match crossfade {
                    Some(frames) => {
                        outgoing.fade.to(0.0, frames);
                        outgoing.fade_then = Then::Retire;
                    }
                    None => {
                        outgoing.declick.to(0.0, declick);
                        outgoing.declick_then = Then::Retire;
                    }
                }
            } else if let Some(old) = outgoing.program.take() {
                self.retire(old);
            }
        }
        if let Some(frames) = crossfade {
            let d = &mut self.decks[incoming];
            d.state = DeckState::Playing;
            d.fade = Ramp::at(0.0);
            d.fade.to(1.0, frames);
        }
        self.active = incoming;
    }

    fn apply_command(&mut self, cmd: Command) {
        let declick = self.declick;
        match cmd {
            Command::Load(p) => return self.swap_in(p, None),
            Command::Crossfade(p, frames) => return self.swap_in(p, Some(frames)),
            _ => {}
        }
        let deck = &mut self.decks[self.active];
        let Some(program) = deck.program.as_deref_mut() else {
            return;
        };
        let playing = deck.state == DeckState::Playing;
        match cmd {
            Command::Load(_) | Command::Crossfade(..) => {}
            Command::Play { count_in } => {
                if playing {
                    // Pressed again while stopping: keep playing.
                    if matches!(deck.declick_then, Then::Stop | Then::Pause) {
                        deck.declick.to(1.0, declick);
                        deck.declick_then = Then::Nothing;
                    }
                    if deck.fade_then == Then::Stop {
                        deck.fade.to(1.0, declick);
                        deck.fade_then = Then::Nothing;
                    }
                    return;
                }
                let count_in = count_in.then(|| program.count_in_frames()).flatten();
                deck.halt(DeckState::Playing);
                if let Some(frames) = count_in {
                    deck.count_in_to = Some(deck.pos);
                    deck.pos -= frames;
                } else if deck.pos > 0 {
                    deck.declick = Ramp::at(0.0);
                    deck.declick.to(1.0, declick);
                }
            }
            Command::Pause => {
                if playing {
                    deck.declick.to(0.0, declick);
                    deck.declick_then = Then::Pause;
                }
            }
            Command::Stop => {
                if playing {
                    deck.declick.to(0.0, declick);
                    deck.declick_then = Then::Stop;
                } else {
                    deck.halt(DeckState::Stopped);
                    deck.pos = 0;
                }
            }
            Command::Seek(to) => {
                if playing {
                    deck.declick.to(0.0, declick);
                    deck.declick_then = Then::Seek(to);
                } else {
                    deck.pos = to;
                    deck.count_in_to = None;
                }
            }
            Command::FadeIn(frames) => {
                if !playing {
                    deck.halt(DeckState::Playing);
                    deck.fade = Ramp::at(0.0);
                }
                deck.fade.to(1.0, frames);
                deck.fade_then = Then::Nothing;
            }
            Command::FadeOut(frames) => {
                if playing {
                    deck.fade.to(0.0, frames);
                    deck.fade_then = Then::Stop;
                }
            }
            Command::Loop(region) => deck.loop_region = region,
            Command::Jump { to, at } => {
                if !playing {
                    deck.pos = to;
                    deck.count_in_to = None;
                    return;
                }
                let bar_line = match (at, deck.count_in_to, program.grid) {
                    (JumpAt::Now, ..) | (JumpAt::NextBar, None, None) => None,
                    (JumpAt::NextBar, Some(start), _) => Some(start),
                    (JumpAt::NextBar, None, Some(grid)) => Some(grid.next_bar(deck.pos)),
                };
                match bar_line {
                    Some(at) => deck.bar_jump = Some((at, to)),
                    None => {
                        deck.declick.to(0.0, declick);
                        deck.declick_then = Then::Seek(to);
                    }
                }
            }
            Command::Track(i, param) => {
                if let Some(t) = program.tracks.get_mut(i) {
                    match param {
                        TrackParam::GainDb(db) => t.gain_db = db,
                        TrackParam::Pan(p) => t.pan = p,
                        TrackParam::Mute(m) => t.mute = m,
                        TrackParam::Solo(s) => t.solo = s,
                        TrackParam::Bus(b) => t.bus = b,
                    }
                    program.retarget();
                }
            }
            Command::Click(param) => {
                let ramp = program.param_ramp;
                let click = &mut program.click;
                match param {
                    ClickParam::Enabled(on) => click.enabled = on,
                    ClickParam::GainDb(db) => click.gain.to(db_to_gain(db), ramp),
                    ClickParam::Bus(b) => click.bus = b,
                }
            }
        }
    }

    fn render_deck(&mut self, d: usize, out: &mut [f32], channels: usize, n: usize) {
        let active = d == self.active;
        let Self {
            decks,
            track_pos,
            click_pos,
            gain,
            events,
            declick,
            unrouted,
            retired,
            ..
        } = self;
        let deck = &mut decks[d];
        if deck.state != DeckState::Playing || deck.program.is_none() {
            return;
        }
        let (Some(tp), Some(cp), Some(g)) = (
            track_pos.get_mut(..n),
            click_pos.get_mut(..n),
            gain.get_mut(..n),
        ) else {
            return;
        };
        let ctx = Ctx {
            active,
            declick: *declick,
            events,
        };
        timeline(deck, tp, cp, g, &ctx);
        if let Some(program) = deck.program.as_deref_mut() {
            mix(program, tp, cp, g, out, channels, unrouted);
        }
        if deck.retire {
            deck.retire = false;
            if let Some(p) = deck.program.take() {
                // See `Renderer::retire`.
                let _ = retired.try_send(p);
            }
        }
    }

    fn publish(&self, out: &[f32], channels: usize) {
        let deck = &self.decks[self.active];
        let state = match deck.state {
            DeckState::Stopped => 0,
            DeckState::Playing => 1,
            DeckState::Paused => 2,
        };
        let mut flags = state & STATE_MASK;
        if deck.count_in_to.is_some() {
            flags |= COUNTING_IN;
        }
        if deck.loop_region.is_some() {
            flags |= LOOPING;
        }
        if self.unrouted {
            flags |= UNROUTED;
        }
        let s = &self.shared;
        s.flags.store(flags, Ordering::Relaxed);
        s.position.store(deck.pos, Ordering::Relaxed);
        s.song.store(deck.id(), Ordering::Relaxed);
        let metered = channels.min(s.meters.len());
        for frame in out.chunks_exact(channels) {
            for (sample, meter) in frame.iter().zip(&s.meters).take(metered) {
                let peak = sample.abs();
                if peak.is_finite() && peak > 0.0 {
                    meter.fetch_max(peak.to_bits(), Ordering::Relaxed);
                }
            }
        }
    }
}

/// Walk the deck through `tp.len()` frames.
fn timeline(deck: &mut Deck, tp: &mut [i64], cp: &mut [i64], g: &mut [f32], ctx: &Ctx<'_>) {
    let (id, length, click_on) = match deck.program.as_deref() {
        Some(p) => (p.id, p.length, p.click.enabled && p.grid.is_some()),
        None => (0, Some(0), false),
    };
    for ((tp, cp), g) in tp.iter_mut().zip(cp.iter_mut()).zip(g.iter_mut()) {
        *tp = SILENT;
        *cp = SILENT;
        *g = 0.0;
        if deck.state != DeckState::Playing {
            continue;
        }
        if let Some((at, to)) = deck.bar_jump
            && deck.pos >= at
        {
            deck.pos = to;
            deck.bar_jump = None;
        }
        if let Some((start, end)) = deck.loop_region
            && deck.pos == end
        {
            deck.pos = start;
        }
        if deck.count_in_to.is_none() && length.is_some_and(|l| deck.pos >= l) {
            deck.apply(Then::Stop, false, ctx);
            let _ = ctx.events.try_send(Event::Ended(SongHandle(id)));
            continue;
        }
        *g = deck.fade.tick() * deck.declick.tick();
        match deck.count_in_to {
            Some(start) => *cp = deck.pos - start,
            None => {
                if deck.pos >= 0 {
                    *tp = deck.pos;
                }
                if click_on {
                    *cp = deck.pos;
                }
            }
        }
        deck.pos += 1;
        if let Some(start) = deck.count_in_to
            && deck.pos >= start
        {
            deck.count_in_to = None;
            let _ = ctx.events.try_send(Event::CountInFinished(SongHandle(id)));
        }
        if deck.fade_then != Then::Nothing && deck.fade.settled() && deck.fade.value == 0.0 {
            let then = std::mem::replace(&mut deck.fade_then, Then::Nothing);
            deck.apply(then, true, ctx);
        }
        if deck.declick_then != Then::Nothing && deck.declick.settled() && deck.declick.value == 0.0
        {
            let then = std::mem::replace(&mut deck.declick_then, Then::Nothing);
            deck.apply(then, false, ctx);
        }
    }
}

/// Add the deck's tracks and click into `out`.
fn mix(
    program: &mut Program,
    tp: &[i64],
    cp: &[i64],
    g: &[f32],
    out: &mut [f32],
    channels: usize,
    unrouted: &mut bool,
) {
    for t in &mut program.tracks {
        let route = Route::new(t.bus, channels);
        if matches!(route, Route::None) {
            *unrouted = true;
        }
        for (i, (&p, &gain)) in tp.iter().zip(g).enumerate() {
            let (gl, gr) = (t.left.tick(), t.right.tick());
            if p == SILENT || gain == 0.0 {
                continue;
            }
            let (l, r) = t.clip.frame(usize::try_from(p).unwrap_or(usize::MAX));
            route.add(out, channels, i, l * gl * gain, r * gr * gain);
        }
    }
    let Some(grid) = program.grid else {
        return;
    };
    let click = &mut program.click;
    let route = Route::new(click.bus, channels);
    let beats_per_bar = i64::from(grid.beats_per_bar);
    let mut used = false;
    for (i, (&q, &gain)) in cp.iter().zip(g).enumerate() {
        let level = click.gain.tick();
        if q == SILENT || gain == 0.0 {
            continue;
        }
        used = true;
        let (beat, start) = grid.beat_at(q);
        let sound = if click.accent && beat.rem_euclid(beats_per_bar) == 0 {
            &click.accent_sound
        } else {
            &click.beat_sound
        };
        let offset = usize::try_from(q - start).unwrap_or(usize::MAX);
        if let Some(s) = sound.get(offset) {
            let v = s * level * gain;
            route.add(out, channels, i, v, v);
        }
    }
    if used && matches!(route, Route::None) {
        *unrouted = true;
    }
}
