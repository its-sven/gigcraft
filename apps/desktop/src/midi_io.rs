//! MIDI ports on the desktop through midir (CoreMIDI, WinMM/WinRT, ALSA).

use gigcraft_ui_egui::MidiIo;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CLIENT: &str = "GigCraft";

type Inbox = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Opens every input port (so pedals and footswitches work at once) and
/// output ports on first use.
pub struct Midir {
    inputs: BTreeMap<String, MidiInputConnection<()>>,
    outputs: BTreeMap<String, Arc<Mutex<MidiOutputConnection>>>,
    inbox: Inbox,
    clocks: BTreeMap<String, Clock>,
}

/// A MIDI clock thread for one port: 24 ticks per quarter note at `bpm`.
struct Clock {
    /// Beats per minute as f64 bits (changes take effect at the next tick).
    bpm: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
}

impl Drop for Clock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Midir {
    fn connection(&mut self, port: &str) -> Result<Arc<Mutex<MidiOutputConnection>>, String> {
        if let Some(c) = self.outputs.get(port) {
            return Ok(Arc::clone(c));
        }
        let out = MidiOutput::new(CLIENT).map_err(|e| e.to_string())?;
        let p = out
            .ports()
            .into_iter()
            .find(|p| out.port_name(p).is_ok_and(|n| n == port))
            .ok_or_else(|| format!("MIDI port {port} is gone"))?;
        let conn = Arc::new(Mutex::new(
            out.connect(&p, "gigcraft-out").map_err(|e| e.to_string())?,
        ));
        self.outputs.insert(port.to_string(), Arc::clone(&conn));
        Ok(conn)
    }
}

/// Keep the beat: sleep until shortly before each tick, then wait the rest
/// precisely; ticks are scheduled from the start, so they never drift.
fn run_clock(conn: Arc<Mutex<MidiOutputConnection>>, bpm: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let mut next = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        let tempo = f64::from_bits(bpm.load(Ordering::Relaxed)).clamp(20.0, 400.0);
        let tick = Duration::from_secs_f64(60.0 / (tempo * 24.0));
        if let Ok(mut c) = conn.lock()
            && c.send(&[0xF8]).is_err()
        {
            return;
        }
        next += tick;
        let now = Instant::now();
        if next < now {
            next = now; // fell behind (sleep, debugger): start again from here
        }
        // OS sleeps overshoot by a millisecond or more: wake 4 ms early and
        // wait the rest precisely (costs a little CPU, keeps the beat tight).
        let wait = next - now;
        if wait > Duration::from_millis(5) {
            std::thread::sleep(wait - Duration::from_millis(4));
        }
        while Instant::now() < next {
            std::hint::spin_loop();
        }
    }
}

impl Midir {
    pub fn new() -> Self {
        Self {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            inbox: Arc::new(Mutex::new(Vec::new())),
            clocks: BTreeMap::new(),
        }
    }
}

impl MidiIo for Midir {
    fn outputs(&mut self) -> Vec<String> {
        let Ok(out) = MidiOutput::new(CLIENT) else {
            return Vec::new();
        };
        out.ports()
            .iter()
            .filter_map(|p| out.port_name(p).ok())
            .collect()
    }

    fn refresh_inputs(&mut self) -> Vec<String> {
        let Ok(probe) = MidiInput::new(CLIENT) else {
            return Vec::new();
        };
        let names: Vec<(String, midir::MidiInputPort)> = probe
            .ports()
            .into_iter()
            .filter_map(|p| Some((probe.port_name(&p).ok()?, p)))
            .collect();
        self.inputs
            .retain(|name, _| names.iter().any(|(n, _)| n == name));
        for (name, port) in names.iter() {
            if self.inputs.contains_key(name) || name.starts_with(CLIENT) {
                continue;
            }
            let Ok(mut input) = MidiInput::new(CLIENT) else {
                continue;
            };
            input.ignore(midir::Ignore::None);
            let inbox = Arc::clone(&self.inbox);
            let label = name.clone();
            if let Ok(conn) = input.connect(
                port,
                "gigcraft-in",
                move |_, bytes, _| {
                    if let Ok(mut q) = inbox.lock()
                        && q.len() < 10_000
                    {
                        q.push((label.clone(), bytes.to_vec()));
                    }
                },
                (),
            ) {
                self.inputs.insert(name.clone(), conn);
            }
        }
        self.inputs.keys().cloned().collect()
    }

    fn send(&mut self, port: &str, bytes: &[u8]) -> Result<(), String> {
        let conn = self.connection(port)?;
        let result = conn
            .lock()
            .map_err(|_| "MIDI port busy".to_string())
            .and_then(|mut c| c.send(bytes).map_err(|e| e.to_string()));
        if result.is_err() {
            self.outputs.remove(port);
            self.clocks.remove(port);
        }
        result
    }

    fn set_clock(&mut self, port: &str, bpm: Option<f64>) {
        let Some(bpm) = bpm else {
            self.clocks.remove(port);
            return;
        };
        if let Some(c) = self.clocks.get(port) {
            c.bpm.store(bpm.to_bits(), Ordering::Relaxed);
            return;
        }
        let Ok(conn) = self.connection(port) else {
            return;
        };
        let clock = Clock {
            bpm: Arc::new(AtomicU64::new(bpm.to_bits())),
            stop: Arc::new(AtomicBool::new(false)),
        };
        let (b, s) = (Arc::clone(&clock.bpm), Arc::clone(&clock.stop));
        if std::thread::Builder::new()
            .name(format!("gigcraft-clock-{port}"))
            .spawn(move || run_clock(conn, b, s))
            .is_ok()
        {
            self.clocks.insert(port.to_string(), clock);
        }
    }

    fn poll(&mut self) -> Vec<(String, Vec<u8>)> {
        self.inbox
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use midir::os::unix::VirtualInput;

    /// 120 BPM = 48 ticks a second, 20.8 ms apart. Needs CoreMIDI, so local only.
    #[test]
    #[ignore = "needs CoreMIDI (run locally)"]
    fn clock_keeps_time() {
        // CoreMIDI's timestamp of each message (µs), not the callback's arrival time.
        let times: Arc<Mutex<Vec<u64>>> = Arc::default();
        let sink = Arc::clone(&times);
        let input = MidiInput::new("GigCraft clock test").unwrap();
        let _virt = input
            .create_virtual(
                "GigCraft Clock Test",
                move |stamp, bytes, _| {
                    if bytes == [0xF8] {
                        sink.lock().unwrap().push(stamp);
                    }
                },
                (),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let mut io = Midir::new();
        io.set_clock("GigCraft Clock Test", Some(120.0));
        std::thread::sleep(Duration::from_secs(2));
        io.set_clock("GigCraft Clock Test", None);
        std::thread::sleep(Duration::from_millis(100));
        let t = times.lock().unwrap().clone();
        assert!((94..=98).contains(&t.len()), "{} ticks in 2 s", t.len());
        let gaps: Vec<f64> = t
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64 / 1000.0)
            .collect();
        let worst = gaps.iter().map(|g| (g - 20.833).abs()).fold(0.0, f64::max);
        let mut devs: Vec<f64> = gaps.iter().map(|g| (g - 20.833).abs()).collect();
        devs.sort_by(f64::total_cmp);
        println!(
            "{} ticks, worst deviation {worst:.2} ms, median {:.3} ms, 95% {:.3} ms, first gaps {:?}",
            t.len(),
            devs[devs.len() / 2],
            devs[devs.len() * 95 / 100],
            &gaps[..4]
        );
        assert!(worst < 1.0, "worst deviation {worst:.2} ms");
    }
}
