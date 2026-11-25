//! Audio output on the desktop: `gigcraft-audio-cpal` behind the UI's `AudioHost`.

use gigcraft_audio_cpal::AudioOutput;
use gigcraft_ui_egui::{AudioDevice, AudioHost};

#[derive(Default)]
pub struct Cpal {
    output: Option<AudioOutput>,
}

impl AudioHost for Cpal {
    fn devices(&mut self) -> Vec<AudioDevice> {
        gigcraft_audio_cpal::list_devices()
            .unwrap_or_default()
            .into_iter()
            .map(|d| AudioDevice {
                name: d.name,
                channels: d.max_channels,
                is_default: d.is_default,
            })
            .collect()
    }

    fn open(&mut self, device: Option<&str>, channels: Option<u16>) -> Result<String, String> {
        self.output = None;
        let out = gigcraft_audio_cpal::start(device, channels).map_err(|e| e.to_string())?;
        let desc = format!(
            "{}, {} ch, {} Hz",
            out.device_name(),
            out.channels(),
            out.sample_rate()
        );
        self.output = Some(out);
        Ok(desc)
    }

    fn close(&mut self) {
        self.output = None;
    }

    fn controller(&mut self) -> Option<&mut gigcraft_audio::Controller> {
        self.output.as_mut().map(AudioOutput::controller)
    }

    fn errors(&mut self) -> Vec<String> {
        self.output
            .as_ref()
            .map(AudioOutput::errors)
            .unwrap_or_default()
    }
}
