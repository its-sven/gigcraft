//! Desktop audio host for the GigCraft engine.
//!
//! Opens an output device with cpal, moves the engine's [`Renderer`] into
//! the device callback and hands the [`Controller`] back to the app.
//! Everything musical lives in `gigcraft-audio`; this crate only moves
//! samples.
//!
//! ```no_run
//! # fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! for d in gigcraft_audio_cpal::list_devices()? {
//!     println!("{} ({} channels max)", d.name, d.max_channels);
//! }
//! let mut out = gigcraft_audio_cpal::start(None, Some(4))?;
//! let rate = out.sample_rate(); // decode clips at this rate
//! out.controller().stop()?;
//! # Ok(()) }
//! ```

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig, SupportedStreamConfigRange};
use gigcraft_audio::{Controller, EngineConfig, Renderer, engine};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// Frames converted per pass in the callback (the scratch buffer is
/// allocated once, before the stream starts).
const SCRATCH_FRAMES: usize = 2048;

/// The sample rate used when the device leaves the choice to us.
const PREFERRED_RATE: u32 = 48_000;

/// An output device as the system names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
    /// Name to show (and to pass to [`start`]).
    pub name: String,
    /// Stable id, also accepted by [`start`].
    pub id: String,
    /// Most output channels the device offers.
    pub max_channels: u16,
    pub default_channels: u16,
    pub default_sample_rate: u32,
    pub is_default: bool,
}

/// Why the audio output could not be opened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("no audio output device")]
    NoDefaultDevice,
    #[error("no audio output device named {0:?}")]
    NoDevice(String),
    #[error("{device} cannot play {channels} channels")]
    Channels { device: String, channels: u16 },
    #[error("audio device: {0}")]
    Device(String),
}

fn device_error(e: impl std::fmt::Display) -> HostError {
    HostError::Device(e.to_string())
}

/// The output devices of the system's default audio host.
///
/// # Errors
/// The audio system could not be asked.
pub fn list_devices() -> Result<Vec<OutputDevice>, HostError> {
    let host = cpal::default_host();
    let default_id = host.default_output_device().and_then(|d| d.id().ok());
    let mut out = Vec::new();
    for device in host.output_devices().map_err(device_error)? {
        let Ok(id) = device.id() else { continue };
        let max_channels = device
            .supported_output_configs()
            .map(|c| c.map(|r| r.channels()).max().unwrap_or(0))
            .unwrap_or(0);
        let default = device.default_output_config().ok();
        out.push(OutputDevice {
            name: device_name(&device),
            id: id.to_string(),
            max_channels,
            default_channels: default.as_ref().map_or(0, |c| c.channels()),
            default_sample_rate: default.as_ref().map_or(0, |c| c.sample_rate()),
            is_default: default_id.as_ref() == Some(&id),
        });
    }
    Ok(out)
}

fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map_or_else(|_| device.to_string(), |d| d.name().to_string())
}

/// A running output stream with the engine inside.
///
/// Dropping it stops the sound. On some platforms the stream must be dropped
/// on the thread that started it, so keep it where it was made.
pub struct AudioOutput {
    _stream: cpal::Stream,
    controller: Controller,
    device: String,
    sample_rate: u32,
    channels: u16,
    errors: Receiver<String>,
}

impl std::fmt::Debug for AudioOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioOutput")
            .field("device", &self.device)
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl AudioOutput {
    /// Drive the engine: load songs, play, stop, mix.
    pub fn controller(&mut self) -> &mut Controller {
        &mut self.controller
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device
    }

    /// Decode clips at this rate.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Output channels of the stream: buses 0 … channels/2 − 1 are audible.
    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Stream errors since the last call (device lost, buffer underruns).
    #[must_use]
    pub fn errors(&self) -> Vec<String> {
        self.errors.try_iter().collect()
    }
}

/// Open `device` (a name or id from [`list_devices`]; `None` for the system
/// default) with at least `channels` outputs (`None` for the device's
/// default) and start the engine on it.
///
/// # Errors
/// No such device, no configuration with enough channels in a supported
/// sample format, or the stream could not start.
pub fn start(device: Option<&str>, channels: Option<u16>) -> Result<AudioOutput, HostError> {
    let host = cpal::default_host();
    let device = match device {
        None => host
            .default_output_device()
            .ok_or(HostError::NoDefaultDevice)?,
        Some(wanted) => host
            .output_devices()
            .map_err(device_error)?
            .find(|d| device_name(d) == wanted || d.id().is_ok_and(|id| id.to_string() == wanted))
            .ok_or_else(|| HostError::NoDevice(wanted.to_string()))?,
    };
    let name = device_name(&device);
    let default = device.default_output_config().ok();
    let wanted = channels
        .or_else(|| default.as_ref().map(cpal::SupportedStreamConfig::channels))
        .unwrap_or(2)
        .max(1);
    let rate = default
        .as_ref()
        .map_or(PREFERRED_RATE, cpal::SupportedStreamConfig::sample_rate);
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(device_error)?
        .collect();
    let (config, format) = choose_config(&ranges, wanted, rate).ok_or(HostError::Channels {
        device: name.clone(),
        channels: wanted,
    })?;
    let (controller, renderer) = engine(EngineConfig::new(config.sample_rate));
    let (err_tx, errors) = sync_channel(64);
    let stream = match format {
        SampleFormat::I16 => build::<i16>(&device, &config, renderer, err_tx),
        SampleFormat::I32 => build::<i32>(&device, &config, renderer, err_tx),
        _ => build::<f32>(&device, &config, renderer, err_tx),
    }?;
    stream.play().map_err(device_error)?;
    Ok(AudioOutput {
        _stream: stream,
        controller,
        device: name,
        sample_rate: config.sample_rate,
        channels: config.channels,
        errors,
    })
}

/// The best configuration for `channels` outputs: the fewest channels that
/// are enough, then `f32` over `i32` over `i16`, at `rate` when the device
/// can (else 48 kHz, else the nearest it offers).
#[must_use]
pub fn choose_config(
    ranges: &[SupportedStreamConfigRange],
    channels: u16,
    rate: u32,
) -> Option<(StreamConfig, SampleFormat)> {
    let rank = |f: SampleFormat| match f {
        SampleFormat::F32 => Some(0),
        SampleFormat::I32 => Some(1),
        SampleFormat::I16 => Some(2),
        _ => None,
    };
    let best = ranges
        .iter()
        .filter(|r| r.channels() >= channels)
        .filter_map(|r| Some((r.channels() - channels, rank(r.sample_format())?, r)))
        .min_by_key(|(extra, rank, _)| (*extra, *rank))?
        .2;
    let (min, max) = (best.min_sample_rate(), best.max_sample_rate());
    let rate = if (min..=max).contains(&rate) {
        rate
    } else if (min..=max).contains(&PREFERRED_RATE) {
        PREFERRED_RATE
    } else {
        rate.clamp(min, max)
    };
    Some((
        StreamConfig {
            channels: best.channels(),
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        },
        best.sample_format(),
    ))
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut renderer: Renderer,
    errors: SyncSender<String>,
) -> Result<cpal::Stream, HostError>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = usize::from(config.channels);
    // Allocated here, never in the callback.
    let mut scratch = vec![0.0f32; SCRATCH_FRAMES * channels];
    device
        .build_output_stream(
            *config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for chunk in data.chunks_mut(scratch.len()) {
                    let Some(mixed) = scratch.get_mut(..chunk.len()) else {
                        break;
                    };
                    renderer.render(mixed, channels);
                    for (dst, src) in chunk.iter_mut().zip(mixed.iter()) {
                        *dst = T::from_sample(src.clamp(-1.0, 1.0));
                    }
                }
            },
            move |e| {
                // Called off the audio path; formatting is fine here.
                let _ = errors.try_send(e.to_string());
            },
            None,
        )
        .map_err(device_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cpal::SupportedBufferSize;

    fn range(channels: u16, min: u32, max: u32, f: SampleFormat) -> SupportedStreamConfigRange {
        SupportedStreamConfigRange::new(channels, min, max, SupportedBufferSize::Unknown, f)
    }

    #[test]
    fn picks_the_smallest_fitting_float_config() {
        let ranges = [
            range(2, 44_100, 96_000, SampleFormat::F32),
            range(8, 44_100, 96_000, SampleFormat::I16),
            range(4, 44_100, 96_000, SampleFormat::I16),
            range(4, 44_100, 96_000, SampleFormat::F32),
            range(4, 44_100, 96_000, SampleFormat::U8),
        ];
        let (c, f) = choose_config(&ranges, 4, 44_100).unwrap();
        assert_eq!(
            (c.channels, c.sample_rate, f),
            (4, 44_100, SampleFormat::F32)
        );
        let (c, f) = choose_config(&ranges, 6, 44_100).unwrap();
        assert_eq!((c.channels, f), (8, SampleFormat::I16));
        assert!(choose_config(&ranges, 10, 44_100).is_none());
        assert!(choose_config(&[range(2, 8_000, 48_000, SampleFormat::U8)], 2, 48_000).is_none());
    }

    #[test]
    fn falls_back_to_48k_or_the_nearest_rate() {
        let ranges = [range(2, 32_000, 48_000, SampleFormat::F32)];
        assert_eq!(
            choose_config(&ranges, 2, 96_000).unwrap().0.sample_rate,
            48_000
        );
        let ranges = [range(2, 88_200, 192_000, SampleFormat::F32)];
        assert_eq!(
            choose_config(&ranges, 2, 44_100).unwrap().0.sample_rate,
            88_200
        );
    }

    /// Needs a real output device, so it is not run in CI:
    /// `cargo test -p gigcraft-audio-cpal -- --ignored`. Plays silence.
    #[test]
    #[ignore = "needs an audio output device"]
    fn runs_on_the_default_device() {
        let devices = list_devices().unwrap();
        assert!(devices.iter().any(|d| d.is_default), "{devices:?}");
        let mut out = start(None, None).unwrap();
        let song = gigcraft_audio::SongSetup::default(); // no tracks, no click: silence
        out.controller().load(song).unwrap();
        out.controller().play(false).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let status = out.controller().status();
        assert!(status.position_frames > 0, "{status:?} {:?}", out.errors());
        out.controller().stop().unwrap();
    }
}
