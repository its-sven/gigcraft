//! Audio files decoded into memory at the host's sample rate.

use std::sync::Arc;

/// Most samples (frames × channels) one file may decode to before it is
/// resampled: 2^28 `f32`, 1 GiB, about 46 minutes of 48 kHz stereo.
pub const MAX_CLIP_SAMPLES: u64 = 1 << 28;

/// The file extensions [`Clip::decode`] reads (lower case, no dot).
pub const AUDIO_EXTENSIONS: &[&str] = craft_audio::DECODE_EXTENSIONS;

/// Why an audio file could not be used.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("cannot decode audio: {0}")]
    Decode(#[from] craft_audio::AudioError),
    #[error("sample rate {0} Hz is not supported")]
    SampleRate(u32),
    #[error("the file has no audio")]
    Empty,
    #[error("track {file:?}: {source}")]
    Track {
        file: String,
        #[source]
        source: Box<LoadError>,
    },
}

/// Decoded audio, mono or stereo, at a fixed sample rate. Cheap to clone
/// (shared); the engine never frees one on the audio thread.
#[derive(Debug, Clone)]
pub struct Clip {
    data: Arc<ClipData>,
}

#[derive(Debug)]
struct ClipData {
    rate: u32,
    left: Vec<f32>,
    /// `None` for mono.
    right: Option<Vec<f32>>,
}

impl Clip {
    /// Decode an audio file (WAV, AIFF, FLAC, MP3, AAC/M4A, Ogg and the rest
    /// of [`AUDIO_EXTENSIONS`]) and resample it to `rate`. `ext_hint` is the
    /// file extension, used when the contents do not say what the file is.
    /// Files with more than two channels keep the first two (front left and
    /// right).
    ///
    /// # Errors
    /// A file that cannot be decoded, is longer than [`MAX_CLIP_SAMPLES`], has
    /// no audio, or a rate outside 1 kHz … 1.536 MHz.
    pub fn decode(bytes: &[u8], ext_hint: Option<&str>, rate: u32) -> Result<Self, LoadError> {
        let opts = craft_audio::DecodeOptions::default().with_max_samples(MAX_CLIP_SAMPLES);
        let (_info, buf) = craft_audio::decode_with(bytes, ext_hint, &opts)?;
        Self::from_planar(buf.sample_rate, buf.channels, rate)
    }

    /// Audio the host decoded or generated itself, planar, at `source_rate`,
    /// resampled to `rate`. Channels after the second are dropped.
    ///
    /// # Errors
    /// No channels or no frames, or a rate outside 1 kHz … 1.536 MHz.
    pub fn from_planar(
        source_rate: u32,
        mut channels: Vec<Vec<f32>>,
        rate: u32,
    ) -> Result<Self, LoadError> {
        for r in [rate, source_rate] {
            if !usable_rate(r) {
                return Err(LoadError::SampleRate(r));
            }
        }
        channels.truncate(2);
        let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
        if frames == 0 {
            return Err(LoadError::Empty);
        }
        for ch in &mut channels {
            ch.truncate(frames);
        }
        let mut channels = if source_rate == rate {
            channels
        } else {
            let views: Vec<&[f32]> = channels.iter().map(Vec::as_slice).collect();
            craft_audio_dsp::offline::resample(&views, source_rate, rate)
                .ok_or(LoadError::SampleRate(source_rate))?
        };
        for ch in &mut channels {
            for s in ch.iter_mut() {
                if !s.is_finite() {
                    *s = 0.0;
                }
            }
        }
        let right = if channels.len() > 1 {
            channels.pop()
        } else {
            None
        };
        let left = channels.pop().ok_or(LoadError::Empty)?;
        if left.is_empty() {
            return Err(LoadError::Empty);
        }
        Ok(Self {
            data: Arc::new(ClipData { rate, left, right }),
        })
    }

    /// Sample rate in Hz.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.data.rate
    }

    /// 1 (mono) or 2 (stereo).
    #[must_use]
    pub fn channels(&self) -> usize {
        if self.data.right.is_some() { 2 } else { 1 }
    }

    /// Length in frames.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.data.left.len()
    }

    /// Length in seconds.
    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / f64::from(self.data.rate.max(1))
    }

    /// Left and right sample of frame `i` (both the same for mono); silence
    /// past the end.
    #[inline]
    pub(crate) fn frame(&self, i: usize) -> (f32, f32) {
        let l = self.data.left.get(i).copied().unwrap_or(0.0);
        let r = match &self.data.right {
            Some(r) => r.get(i).copied().unwrap_or(0.0),
            None => l,
        };
        (l, r)
    }

    /// The samples of one channel (0 left, 1 right; mono returns the same for both).
    #[must_use]
    pub fn samples(&self, channel: usize) -> &[f32] {
        match (&self.data.right, channel) {
            (Some(r), 1) => r,
            _ => &self.data.left,
        }
    }
}

fn usable_rate(rate: u32) -> bool {
    (1_000..=1_536_000).contains(&rate)
}
