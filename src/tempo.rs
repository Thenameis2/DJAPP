use std::{error::Error, fmt};

use signalsmith_stretch::Stretch;

use crate::media::decode::PcmChunk;

pub const MIN_TEMPO_PERCENT: f32 = -16.0;
pub const MAX_TEMPO_PERCENT: f32 = 16.0;
pub const MIN_PITCH_SEMITONES: f32 = -12.0;
pub const MAX_PITCH_SEMITONES: f32 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoSettings {
    pub tempo_percent: f32,
    pub key_lock: bool,
    pub pitch_semitones: f32,
}

impl Default for TempoSettings {
    fn default() -> Self {
        Self {
            tempo_percent: 0.0,
            key_lock: false,
            pitch_semitones: 0.0,
        }
    }
}

impl TempoSettings {
    pub fn tempo_ratio(self) -> f64 {
        1.0 + f64::from(self.tempo_percent) / 100.0
    }

    fn transpose_semitones(self) -> f32 {
        let vinyl_shift = if self.key_lock {
            0.0
        } else {
            12.0 * (self.tempo_ratio() as f32).log2()
        };
        self.pitch_semitones + vinyl_shift
    }

    pub fn is_neutral(self) -> bool {
        self.tempo_percent.abs() < f32::EPSILON && self.pitch_semitones.abs() < f32::EPSILON
    }

    pub fn uses_varispeed(self) -> bool {
        self.tempo_percent.abs() >= f32::EPSILON
            && !self.key_lock
            && self.pitch_semitones.abs() < f32::EPSILON
    }

    pub fn validate(self) -> Result<Self, TempoError> {
        if !self.tempo_percent.is_finite()
            || !(MIN_TEMPO_PERCENT..=MAX_TEMPO_PERCENT).contains(&self.tempo_percent)
            || !self.pitch_semitones.is_finite()
            || !(MIN_PITCH_SEMITONES..=MAX_PITCH_SEMITONES).contains(&self.pitch_semitones)
        {
            return Err(TempoError::InvalidSetting);
        }
        Ok(self)
    }
}

pub struct TempoProcessor {
    stretch: Stretch,
    sample_rate: u32,
    channels: usize,
    settings: TempoSettings,
    requested_output_exact: f64,
    requested_output_frames: u64,
    emitted_output_frames: u64,
    startup_trim_remaining: usize,
    varispeed_previous_frame: Vec<f32>,
    varispeed_position: f64,
}

impl TempoProcessor {
    pub fn new(
        sample_rate: u32,
        channels: usize,
        settings: TempoSettings,
    ) -> Result<Self, TempoError> {
        if sample_rate == 0 || channels == 0 || channels > u32::MAX as usize {
            return Err(TempoError::InvalidFormat);
        }
        let settings = settings.validate()?;
        let mut stretch = Stretch::preset_default(channels as u32, sample_rate);
        stretch.set_transpose_factor_semitones(settings.transpose_semitones(), None);
        let startup_trim_remaining = stretch.output_latency();
        Ok(Self {
            stretch,
            sample_rate,
            channels,
            settings,
            requested_output_exact: 0.0,
            requested_output_frames: 0,
            emitted_output_frames: 0,
            startup_trim_remaining,
            varispeed_previous_frame: Vec::with_capacity(channels),
            varispeed_position: 0.0,
        })
    }

    pub fn settings(&self) -> TempoSettings {
        self.settings
    }

    pub fn latency_frames(&self) -> usize {
        if self.settings.is_neutral() || self.settings.uses_varispeed() {
            0
        } else {
            self.stretch.input_latency() + self.stretch.output_latency()
        }
    }

    pub fn set_settings(&mut self, settings: TempoSettings) -> Result<(), TempoError> {
        let settings = settings.validate()?;
        let previous_varispeed = self.settings.uses_varispeed();
        let previous_stretch = !self.settings.is_neutral() && !previous_varispeed;
        let next_varispeed = settings.uses_varispeed();
        let next_stretch = !settings.is_neutral() && !next_varispeed;
        self.settings = settings;
        if previous_varispeed != next_varispeed {
            self.reset_varispeed();
        }
        if previous_stretch != next_stretch && next_stretch {
            self.reset_stretch();
        }
        self.stretch
            .set_transpose_factor_semitones(settings.transpose_semitones(), None);
        Ok(())
    }

    pub fn reset(&mut self) {
        self.reset_varispeed();
        self.reset_stretch();
    }

    fn reset_stretch(&mut self) {
        self.stretch.reset();
        self.stretch
            .set_transpose_factor_semitones(self.settings.transpose_semitones(), None);
        self.requested_output_exact = 0.0;
        self.requested_output_frames = 0;
        self.emitted_output_frames = 0;
        self.startup_trim_remaining = self.stretch.output_latency();
    }

    fn reset_varispeed(&mut self) {
        self.varispeed_previous_frame.clear();
        self.varispeed_position = 0.0;
    }

    pub fn process(
        &mut self,
        chunk: PcmChunk,
        mut output: Vec<f32>,
    ) -> Result<(Option<PcmChunk>, Vec<f32>), TempoError> {
        self.validate_chunk(&chunk)?;
        if self.settings.is_neutral() {
            return Ok((Some(chunk), output));
        }
        if self.settings.uses_varispeed() {
            return self.process_varispeed(chunk, output);
        }
        let input_frames = chunk.frames();
        self.requested_output_exact += input_frames as f64 / self.settings.tempo_ratio();
        let target_requested = self.requested_output_exact.round() as u64;
        let output_frames = target_requested.saturating_sub(self.requested_output_frames) as usize;
        self.requested_output_frames = target_requested;

        output.clear();
        output.resize(output_frames * self.channels, 0.0);
        self.stretch.process(&chunk.samples, &mut output);
        let input_buffer = chunk.samples;
        let output = self.finish_output(output);
        Ok((output, input_buffer))
    }

    pub fn flush(&mut self, mut output: Vec<f32>) -> Option<PcmChunk> {
        if self.settings.is_neutral() {
            return None;
        }
        if self.settings.uses_varispeed() {
            if self.varispeed_previous_frame.is_empty() {
                return None;
            }
            output.clear();
            output.extend_from_slice(&self.varispeed_previous_frame);
            self.reset_varispeed();
            return Some(PcmChunk {
                samples: output,
                sample_rate: self.sample_rate,
                channels: self.channels,
            });
        }
        output.clear();
        output.resize(self.stretch.output_latency() * self.channels, 0.0);
        self.stretch.flush(&mut output);
        self.finish_output(output)
    }

    fn finish_output(&mut self, mut samples: Vec<f32>) -> Option<PcmChunk> {
        let frames = samples.len() / self.channels;
        let trim = self.startup_trim_remaining.min(frames);
        self.startup_trim_remaining -= trim;
        if trim > 0 {
            samples.drain(..trim * self.channels);
        }

        let remaining = self
            .requested_output_frames
            .saturating_sub(self.emitted_output_frames) as usize;
        let useful_frames = (samples.len() / self.channels).min(remaining);
        samples.truncate(useful_frames * self.channels);
        if useful_frames == 0 {
            return None;
        }
        self.emitted_output_frames += useful_frames as u64;
        Some(PcmChunk {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }

    fn validate_chunk(&self, chunk: &PcmChunk) -> Result<(), TempoError> {
        if chunk.sample_rate != self.sample_rate || chunk.channels != self.channels {
            return Err(TempoError::FormatChanged);
        }
        Ok(())
    }

    fn process_varispeed(
        &mut self,
        chunk: PcmChunk,
        mut output: Vec<f32>,
    ) -> Result<(Option<PcmChunk>, Vec<f32>), TempoError> {
        let input_frames = chunk.frames();
        let has_previous = !self.varispeed_previous_frame.is_empty();
        let total_frames = input_frames + usize::from(has_previous);
        output.clear();
        output.reserve(
            ((input_frames as f64 / self.settings.tempo_ratio()).ceil() as usize + 2)
                * self.channels,
        );

        while self.varispeed_position + 1.0 < total_frames as f64 {
            let left_index = self.varispeed_position.floor() as usize;
            let fraction = (self.varispeed_position - left_index as f64) as f32;
            for channel in 0..self.channels {
                let left = frame_sample(
                    &chunk.samples,
                    &self.varispeed_previous_frame,
                    self.channels,
                    has_previous,
                    left_index,
                    channel,
                );
                let right = frame_sample(
                    &chunk.samples,
                    &self.varispeed_previous_frame,
                    self.channels,
                    has_previous,
                    left_index + 1,
                    channel,
                );
                output.push(left + (right - left) * fraction);
            }
            self.varispeed_position += self.settings.tempo_ratio();
        }

        self.varispeed_position -= total_frames.saturating_sub(1) as f64;
        self.varispeed_previous_frame.clear();
        self.varispeed_previous_frame.extend_from_slice(
            &chunk.samples[(input_frames - 1) * self.channels..input_frames * self.channels],
        );
        let input_buffer = chunk.samples;
        let processed = (!output.is_empty()).then_some(PcmChunk {
            samples: output,
            sample_rate: self.sample_rate,
            channels: self.channels,
        });
        Ok((processed, input_buffer))
    }
}

fn frame_sample(
    samples: &[f32],
    previous: &[f32],
    channels: usize,
    has_previous: bool,
    frame: usize,
    channel: usize,
) -> f32 {
    if has_previous && frame == 0 {
        previous[channel]
    } else {
        let source_frame = frame - usize::from(has_previous);
        samples[source_frame * channels + channel]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempoError {
    InvalidSetting,
    InvalidFormat,
    FormatChanged,
}

impl fmt::Display for TempoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSetting => formatter.write_str("tempo or pitch setting is out of range"),
            Self::InvalidFormat => formatter.write_str("tempo processor audio format is invalid"),
            Self::FormatChanged => {
                formatter.write_str("audio format changed during tempo processing")
            }
        }
    }
}

impl Error for TempoError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize) -> PcmChunk {
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let sample = (std::f32::consts::TAU * 440.0 * frame as f32 / 48_000.0).sin();
            samples.extend_from_slice(&[sample, sample]);
        }
        PcmChunk {
            samples,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    fn chirp(start_frame: usize, frames: usize) -> PcmChunk {
        let mut samples = Vec::with_capacity(frames * 2);
        for frame in start_frame..start_frame + frames {
            let time = frame as f32 / 48_000.0;
            let frequency = 180.0 + 70.0 * time;
            let sample = (std::f32::consts::TAU * frequency * time).sin() * 0.5;
            samples.extend_from_slice(&[sample, sample]);
        }
        PcmChunk {
            samples,
            sample_rate: 48_000,
            channels: 2,
        }
    }

    #[test]
    fn settings_validate_approved_ranges() {
        assert!(TempoSettings::default().validate().is_ok());
        assert!(TempoSettings {
            tempo_percent: 16.1,
            ..TempoSettings::default()
        }
        .validate()
        .is_err());
        assert!(TempoSettings {
            pitch_semitones: -12.1,
            ..TempoSettings::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn processor_emits_expected_duration_after_flush() {
        let mut processor = TempoProcessor::new(
            48_000,
            2,
            TempoSettings {
                tempo_percent: -16.0,
                ..TempoSettings::default()
            },
        )
        .unwrap();
        let input_frames = 48_000;
        let mut emitted = 0;
        let (chunk, _) = processor.process(tone(input_frames), Vec::new()).unwrap();
        emitted += chunk.map(|value| value.frames()).unwrap_or(0);
        emitted += processor
            .flush(Vec::new())
            .map(|value| value.frames())
            .unwrap_or(0);
        let expected = (input_frames as f64 / 0.84).round() as usize;
        assert_eq!(emitted, expected);
    }

    #[test]
    fn reset_preserves_controls_and_restarts_latency_accounting() {
        let settings = TempoSettings {
            tempo_percent: 8.0,
            key_lock: false,
            pitch_semitones: 3.0,
        };
        let mut processor = TempoProcessor::new(48_000, 2, settings).unwrap();
        processor.process(tone(4096), Vec::new()).unwrap();
        processor.reset();
        assert_eq!(processor.settings(), settings);
        assert_eq!(processor.latency_frames(), 5760);
    }

    #[test]
    fn rapid_parameter_changes_keep_output_finite() {
        let mut processor = TempoProcessor::new(48_000, 2, TempoSettings::default()).unwrap();
        let mut output_buffer = Vec::new();
        for index in 0..100 {
            let settings = TempoSettings {
                tempo_percent: -16.0 + (index % 33) as f32,
                key_lock: index % 2 == 0,
                pitch_semitones: -12.0 + (index % 25) as f32,
            };
            processor.set_settings(settings).unwrap();
            let (output, input_buffer) = processor.process(tone(512), output_buffer).unwrap();
            output_buffer = input_buffer;
            if let Some(chunk) = output {
                assert!(chunk.samples.iter().all(|sample| sample.is_finite()));
            }
        }
        if let Some(chunk) = processor.flush(output_buffer) {
            assert!(chunk.samples.iter().all(|sample| sample.is_finite()));
        }
    }

    #[test]
    fn neutral_settings_bypass_processing_without_changing_samples() {
        let mut processor = TempoProcessor::new(48_000, 2, TempoSettings::default()).unwrap();
        let input = tone(48_000);
        let expected = input.clone();
        let (output, recycled) = processor.process(input, Vec::new()).unwrap();
        assert_eq!(output.unwrap(), expected);
        assert!(recycled.is_empty());
        assert_eq!(processor.latency_frames(), 0);
        assert!(processor.flush(Vec::new()).is_none());
    }

    #[test]
    fn non_neutral_settings_report_processor_latency() {
        let mut processor = TempoProcessor::new(48_000, 2, TempoSettings::default()).unwrap();
        processor
            .set_settings(TempoSettings {
                tempo_percent: 8.0,
                key_lock: true,
                ..TempoSettings::default()
            })
            .unwrap();
        assert_eq!(processor.latency_frames(), 5760);
        processor.set_settings(TempoSettings::default()).unwrap();
        assert_eq!(processor.latency_frames(), 0);
    }

    #[test]
    fn varispeed_changes_rate_without_repeating_music_like_windows() {
        let mut processor = TempoProcessor::new(
            48_000,
            2,
            TempoSettings {
                tempo_percent: 8.0,
                key_lock: false,
                pitch_semitones: 0.0,
            },
        )
        .unwrap();
        let mut recycled = Vec::new();
        let mut output = Vec::new();
        for start in (0..48_000 * 8).step_by(1024) {
            let frames = 1024.min(48_000 * 8 - start);
            let (chunk, input) = processor.process(chirp(start, frames), recycled).unwrap();
            recycled = input;
            if let Some(chunk) = chunk {
                output.extend_from_slice(&chunk.samples);
            }
        }
        if let Some(chunk) = processor.flush(recycled) {
            output.extend_from_slice(&chunk.samples);
        }

        let expected = (48_000.0_f64 * 8.0 / 1.08).round() as usize;
        assert!((output.len() / 2).abs_diff(expected) <= 2);
        assert_eq!(processor.latency_frames(), 0);
    }

    #[test]
    fn streamed_chirp_does_not_repeat_output_windows() {
        let mut processor = TempoProcessor::new(
            48_000,
            2,
            TempoSettings {
                tempo_percent: 8.0,
                ..TempoSettings::default()
            },
        )
        .unwrap();
        let mut recycled = Vec::new();
        let mut output = Vec::new();
        for start in (0..48_000 * 8).step_by(1024) {
            let frames = 1024.min(48_000 * 8 - start);
            let (chunk, input) = processor.process(chirp(start, frames), recycled).unwrap();
            recycled = input;
            if let Some(chunk) = chunk {
                output.extend_from_slice(&chunk.samples);
            }
        }
        if let Some(chunk) = processor.flush(recycled) {
            output.extend_from_slice(&chunk.samples);
        }

        let window_samples = 2048 * 2;
        let windows: Vec<&[f32]> = output.chunks_exact(window_samples).collect();
        for (index, window) in windows.iter().enumerate() {
            for previous in windows[..index].iter().rev().take(12) {
                let mean_difference = window
                    .iter()
                    .zip(previous.iter())
                    .map(|(left, right)| (left - right).abs())
                    .sum::<f32>()
                    / window.len() as f32;
                assert!(
                    mean_difference > 0.0001,
                    "streaming processor repeated an earlier output window"
                );
            }
        }
    }
}
