//! MIDI ports on the desktop through midir (CoreMIDI, WinMM/WinRT, ALSA).

use gigcraft_ui_egui::MidiIo;
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const CLIENT: &str = "GigCraft";

type Inbox = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// Opens every input port (so pedals and footswitches work at once) and
/// output ports on first use.
pub struct Midir {
    inputs: BTreeMap<String, MidiInputConnection<()>>,
    outputs: BTreeMap<String, MidiOutputConnection>,
    inbox: Inbox,
}

impl Midir {
    pub fn new() -> Self {
        Self {
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            inbox: Arc::new(Mutex::new(Vec::new())),
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
        if !self.outputs.contains_key(port) {
            let out = MidiOutput::new(CLIENT).map_err(|e| e.to_string())?;
            let p = out
                .ports()
                .into_iter()
                .find(|p| out.port_name(p).is_ok_and(|n| n == port))
                .ok_or_else(|| format!("MIDI port {port} is gone"))?;
            let conn = out.connect(&p, "gigcraft-out").map_err(|e| e.to_string())?;
            self.outputs.insert(port.to_string(), conn);
        }
        let result = self.outputs.get_mut(port).map(|c| c.send(bytes));
        match result {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => {
                self.outputs.remove(port);
                Err(e.to_string())
            }
            None => Err(format!("MIDI port {port} is not open")),
        }
    }

    fn poll(&mut self) -> Vec<(String, Vec<u8>)> {
        self.inbox
            .lock()
            .map(|mut q| std::mem::take(&mut *q))
            .unwrap_or_default()
    }
}
