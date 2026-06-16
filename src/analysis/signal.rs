pub const ANALYSIS_SAMPLE_RATE: u32 = 22_050;

pub fn analysis_frame_to_source_frame(analysis_frame: u64, source_rate: u32) -> u64 {
    let numerator =
        u128::from(analysis_frame) * u128::from(source_rate) + u128::from(ANALYSIS_SAMPLE_RATE / 2);
    (numerator / u128::from(ANALYSIS_SAMPLE_RATE)).min(u128::from(u64::MAX)) as u64
}

pub struct AnalysisSignalBuilder {
    source_rate: u32,
    channels: usize,
    next_source_position: f64,
    source_frame_index: u64,
    previous_mono: Option<f32>,
    output: Vec<f32>,
    output_frames: u64,
}

impl AnalysisSignalBuilder {
    pub fn new(source_rate: u32, channels: usize) -> Result<Self, String> {
        if source_rate == 0 || channels == 0 {
            return Err("analysis signal format is invalid".to_string());
        }
        Ok(Self {
            source_rate,
            channels,
            next_source_position: 0.0,
            source_frame_index: 0,
            previous_mono: None,
            output: Vec::new(),
            output_frames: 0,
        })
    }

    pub fn push_interleaved(&mut self, samples: &[f32]) -> Result<(), String> {
        if !samples.len().is_multiple_of(self.channels)
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err("analysis signal input is malformed".to_string());
        }
        let source_step = f64::from(self.source_rate) / f64::from(ANALYSIS_SAMPLE_RATE);
        for frame in samples.chunks_exact(self.channels) {
            let mono = frame.iter().copied().sum::<f32>() / self.channels as f32;
            if let Some(previous) = self.previous_mono {
                let right_position = self.source_frame_index as f64;
                let left_position = right_position - 1.0;
                while self.next_source_position <= right_position {
                    let fraction =
                        (self.next_source_position - left_position).clamp(0.0, 1.0) as f32;
                    self.output.push(previous + (mono - previous) * fraction);
                    self.output_frames += 1;
                    self.next_source_position += source_step;
                }
            } else {
                self.output.push(mono);
                self.output_frames += 1;
                self.next_source_position = source_step;
            }
            self.previous_mono = Some(mono);
            self.source_frame_index += 1;
        }
        Ok(())
    }

    pub fn take_output(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.output)
    }

    pub fn finish(self) -> Result<Vec<f32>, String> {
        if self.output_frames == 0 {
            return Err("analysis signal is empty".to_string());
        }
        Ok(self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmixes_and_converts_to_fixed_rate() {
        let mut builder = AnalysisSignalBuilder::new(44_100, 2).unwrap();
        let frames = 44_100;
        let mut input = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let value = frame as f32 / frames as f32;
            input.extend_from_slice(&[value, value * 0.5]);
        }
        for chunk in input.chunks(1_002) {
            builder.push_interleaved(chunk).unwrap();
        }
        let output = builder.finish().unwrap();
        assert!(output.len().abs_diff(ANALYSIS_SAMPLE_RATE as usize) <= 1);
        assert!(output.iter().all(|sample| sample.is_finite()));
        assert!((output[1] - 1.5 / 44_100.0).abs() < 0.000_001);
    }

    #[test]
    fn chunk_boundaries_do_not_change_output() {
        let input: Vec<f32> = (0..48_000)
            .flat_map(|frame| {
                let sample = (frame as f32 * 0.013).sin();
                [sample, -sample * 0.25]
            })
            .collect();
        let mut whole = AnalysisSignalBuilder::new(48_000, 2).unwrap();
        whole.push_interleaved(&input).unwrap();
        let whole = whole.finish().unwrap();

        let mut chunked = AnalysisSignalBuilder::new(48_000, 2).unwrap();
        let mut chunked_output = Vec::new();
        for chunk in input.chunks(2_046) {
            chunked.push_interleaved(chunk).unwrap();
            chunked_output.extend(chunked.take_output());
        }
        chunked_output.extend(chunked.finish().unwrap());
        assert_eq!(chunked_output, whole);
    }

    #[test]
    fn analysis_positions_convert_without_accumulated_drift() {
        assert_eq!(analysis_frame_to_source_frame(0, 44_100), 0);
        assert_eq!(analysis_frame_to_source_frame(22_050, 44_100), 44_100);
        assert_eq!(analysis_frame_to_source_frame(11_025, 48_000), 24_000);
        let beat = analysis_frame_to_source_frame(551_250, 44_100);
        assert_eq!(beat, 1_102_500);
    }
}
