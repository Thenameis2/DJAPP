use std::{collections::VecDeque, error::Error};

use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, FixedSync, Indexing, Resampler};

use super::decode::{MediaDecoder, MediaInfo, PcmChunk};

const RESAMPLER_CHUNK_FRAMES: usize = 1024;
const RESAMPLER_SUB_CHUNKS: usize = 4;

pub struct EngineRateDecoder {
    source: MediaDecoder,
    target_rate: u32,
    converter: Option<RateConverter>,
}

impl EngineRateDecoder {
    pub fn new(source: MediaDecoder, target_rate: u32) -> Result<Self, Box<dyn Error>> {
        if target_rate == 0 {
            return Err("target sample rate must be greater than zero".into());
        }
        let converter = if source.info().sample_rate == target_rate {
            None
        } else {
            Some(RateConverter::new(
                source.info().sample_rate,
                target_rate,
                source.info().channels,
            )?)
        };
        Ok(Self {
            source,
            target_rate,
            converter,
        })
    }

    pub fn info(&self) -> &MediaInfo {
        self.source.info()
    }

    pub fn target_rate(&self) -> u32 {
        self.target_rate
    }

    pub fn seek(&mut self, seconds: f64) -> Result<f64, Box<dyn Error>> {
        let actual = self.source.seek(seconds)?;
        if let Some(converter) = self.converter.as_mut() {
            converter.reset();
        }
        Ok(actual)
    }

    pub fn next_chunk_into(
        &mut self,
        buffer: Vec<f32>,
    ) -> Result<Option<PcmChunk>, Box<dyn Error>> {
        let Some(converter) = self.converter.as_mut() else {
            return self.source.next_chunk_into(buffer);
        };

        loop {
            if let Some(mut chunk) = converter.pending.pop_front() {
                if buffer.capacity() >= chunk.samples.len() {
                    let mut reused = buffer;
                    reused.extend_from_slice(&chunk.samples);
                    chunk.samples = reused;
                }
                return Ok(Some(chunk));
            }

            if converter.finished {
                return Ok(None);
            }

            if converter.source_eof {
                converter.flush_next()?;
                continue;
            }

            match self.source.next_chunk()? {
                Some(chunk) => converter.push_source(chunk)?,
                None => {
                    converter.source_eof = true;
                    converter.expected_output_frames = Some(
                        ((converter.source_frames as u128 * self.target_rate as u128)
                            .div_ceil(converter.source_rate as u128))
                            as usize,
                    );
                }
            }
        }
    }
}

struct RateConverter {
    resampler: Fft<f32>,
    source_rate: u32,
    target_rate: u32,
    channels: usize,
    input: Vec<f32>,
    input_offset_frames: usize,
    pending: VecDeque<PcmChunk>,
    delay_remaining: usize,
    source_frames: usize,
    emitted_frames: usize,
    expected_output_frames: Option<usize>,
    source_eof: bool,
    partial_processed: bool,
    finished: bool,
}

impl RateConverter {
    fn new(source_rate: u32, target_rate: u32, channels: usize) -> Result<Self, Box<dyn Error>> {
        let resampler = Fft::<f32>::new(
            source_rate as usize,
            target_rate as usize,
            RESAMPLER_CHUNK_FRAMES,
            RESAMPLER_SUB_CHUNKS,
            channels,
            FixedSync::Input,
        )?;
        let delay_remaining = resampler.output_delay();
        Ok(Self {
            resampler,
            source_rate,
            target_rate,
            channels,
            input: Vec::new(),
            input_offset_frames: 0,
            pending: VecDeque::new(),
            delay_remaining,
            source_frames: 0,
            emitted_frames: 0,
            expected_output_frames: None,
            source_eof: false,
            partial_processed: false,
            finished: false,
        })
    }

    fn reset(&mut self) {
        self.resampler.reset();
        self.input.clear();
        self.input_offset_frames = 0;
        self.pending.clear();
        self.delay_remaining = self.resampler.output_delay();
        self.source_frames = 0;
        self.emitted_frames = 0;
        self.expected_output_frames = None;
        self.source_eof = false;
        self.partial_processed = false;
        self.finished = false;
    }

    fn push_source(&mut self, chunk: PcmChunk) -> Result<(), Box<dyn Error>> {
        if chunk.channels != self.channels || chunk.sample_rate != self.source_rate {
            return Err("decoded audio format changed during resampling".into());
        }
        self.source_frames += chunk.frames();
        self.compact_input();
        self.input.extend_from_slice(&chunk.samples);
        self.process_full_chunks()?;
        Ok(())
    }

    fn process_full_chunks(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            let needed = self.resampler.input_frames_next();
            if self.available_input_frames() < needed {
                return Ok(());
            }
            self.process(needed, None)?;
            self.input_offset_frames += needed;
        }
    }

    fn flush_next(&mut self) -> Result<(), Box<dyn Error>> {
        let expected = self.expected_output_frames.unwrap_or(0);
        if self.emitted_frames >= expected {
            self.finished = true;
            return Ok(());
        }

        if !self.partial_processed {
            let available = self.available_input_frames();
            self.process(available, Some(available))?;
            self.input_offset_frames += available;
            self.partial_processed = true;
        } else {
            self.process(0, Some(0))?;
        }

        if self.emitted_frames >= expected {
            self.finished = true;
        }
        Ok(())
    }

    fn process(
        &mut self,
        valid_input_frames: usize,
        partial_len: Option<usize>,
    ) -> Result<(), Box<dyn Error>> {
        let needed = self.resampler.input_frames_next();
        let input_start = self.input_offset_frames * self.channels;
        let available_samples = valid_input_frames * self.channels;
        let source_slice = &self.input[input_start..input_start + available_samples];
        let mut padded = Vec::new();
        let input_slice = if valid_input_frames < needed {
            padded.resize(needed * self.channels, 0.0);
            padded[..available_samples].copy_from_slice(source_slice);
            padded.as_slice()
        } else {
            source_slice
        };
        let input_adapter = InterleavedSlice::new(input_slice, self.channels, needed)?;
        let output_capacity = self.resampler.output_frames_max();
        let mut output = vec![0.0_f32; output_capacity * self.channels];
        let mut output_adapter =
            InterleavedSlice::new_mut(&mut output, self.channels, output_capacity)?;
        let indexing = Indexing {
            input_offset: 0,
            output_offset: 0,
            active_channels_mask: None,
            partial_len,
        };
        let (_, output_frames) = self.resampler.process_into_buffer(
            &input_adapter,
            &mut output_adapter,
            Some(&indexing),
        )?;
        debug_assert!(partial_len.is_some() || valid_input_frames == needed);
        output.truncate(output_frames * self.channels);
        self.queue_output(output, output_frames);
        Ok(())
    }

    fn queue_output(&mut self, mut output: Vec<f32>, output_frames: usize) {
        let trim = self.delay_remaining.min(output_frames);
        self.delay_remaining -= trim;
        if trim > 0 {
            output.drain(..trim * self.channels);
        }

        let mut useful_frames = output_frames - trim;
        if let Some(expected) = self.expected_output_frames {
            useful_frames = useful_frames.min(expected.saturating_sub(self.emitted_frames));
            output.truncate(useful_frames * self.channels);
        }
        if useful_frames == 0 {
            return;
        }

        self.emitted_frames += useful_frames;
        self.pending.push_back(PcmChunk {
            samples: output,
            sample_rate: self.target_rate,
            channels: self.channels,
        });
    }

    fn available_input_frames(&self) -> usize {
        self.input.len() / self.channels - self.input_offset_frames
    }

    fn compact_input(&mut self) {
        if self.input_offset_frames == 0 {
            return;
        }
        let samples = self.input_offset_frames * self.channels;
        self.input.drain(..samples);
        self.input_offset_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_at(path: &str, target_rate: u32) -> (MediaInfo, Vec<f32>) {
        let source = MediaDecoder::open(path).unwrap();
        let info = source.info().clone();
        let mut decoder = EngineRateDecoder::new(source, target_rate).unwrap();
        let mut samples = Vec::new();
        while let Some(chunk) = decoder.next_chunk_into(Vec::new()).unwrap() {
            assert_eq!(chunk.sample_rate, target_rate);
            assert_eq!(chunk.channels, info.channels);
            samples.extend_from_slice(&chunk.samples);
        }
        (info, samples)
    }

    #[test]
    fn resamples_48k_to_44k1_with_expected_frame_count() {
        let (info, samples) = decode_at("tests/fixtures/audio/tone-48k.wav", 44_100);
        let frames = samples.len() / info.channels;
        assert!((frames as isize - 132_300).abs() <= 1);
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples.iter().any(|sample| sample.abs() > 0.01));
    }

    #[test]
    fn resamples_96k_to_48k_with_expected_frame_count() {
        let (info, samples) = decode_at("tests/fixtures/audio/tone-96k.wav", 48_000);
        let frames = samples.len() / info.channels;
        assert!((frames as isize - 144_000).abs() <= 1);
        assert!(samples.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn seek_resets_resampler_state() {
        let source = MediaDecoder::open("tests/fixtures/audio/tone-48k.wav").unwrap();
        let mut decoder = EngineRateDecoder::new(source, 44_100).unwrap();
        let _ = decoder.next_chunk_into(Vec::new()).unwrap();
        let actual = decoder.seek(1.5).unwrap();
        assert!(actual <= 1.5);

        let mut frames = 0;
        while let Some(chunk) = decoder.next_chunk_into(Vec::new()).unwrap() {
            frames += chunk.frames();
        }
        let expected = ((3.0 - actual) * 44_100.0).round() as isize;
        assert!(
            (frames as isize - expected).abs() <= 2,
            "actual seek={actual}, frames={frames}, expected={expected}"
        );
    }

    #[test]
    fn bypasses_when_rates_match() {
        let source = MediaDecoder::open("tests/fixtures/audio/tone.wav").unwrap();
        let mut decoder = EngineRateDecoder::new(source, 44_100).unwrap();
        let chunk = decoder.next_chunk_into(Vec::new()).unwrap().unwrap();
        assert_eq!(chunk.sample_rate, 44_100);
    }
}
