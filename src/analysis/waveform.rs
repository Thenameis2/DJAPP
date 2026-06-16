use super::cache::{WaveformCache, WaveformLevel};

pub const BASE_BUCKET_FRAMES: u32 = 256;
pub const LEVEL_SCALE: u32 = 4;

#[derive(Clone, Copy, Debug)]
struct BucketStats {
    minimum: f32,
    maximum: f32,
    sum_squares: f64,
    frames: u64,
}

impl BucketStats {
    fn empty() -> Self {
        Self {
            minimum: f32::INFINITY,
            maximum: f32::NEG_INFINITY,
            sum_squares: 0.0,
            frames: 0,
        }
    }

    fn add(&mut self, sample: f32) {
        self.minimum = self.minimum.min(sample);
        self.maximum = self.maximum.max(sample);
        self.sum_squares += f64::from(sample) * f64::from(sample);
        self.frames += 1;
    }

    fn combine(&mut self, other: Self) {
        if other.frames == 0 {
            return;
        }
        self.minimum = self.minimum.min(other.minimum);
        self.maximum = self.maximum.max(other.maximum);
        self.sum_squares += other.sum_squares;
        self.frames += other.frames;
    }

    fn values(self) -> [f32; 3] {
        let rms = (self.sum_squares / self.frames.max(1) as f64).sqrt() as f32;
        [self.minimum, self.maximum, rms]
    }
}

pub struct WaveformBuilder {
    channels: usize,
    current_frames: u32,
    current: Vec<BucketStats>,
    buckets: Vec<Vec<BucketStats>>,
    source_frames: u64,
}

impl WaveformBuilder {
    pub fn new(channels: usize) -> Result<Self, String> {
        if channels == 0 || channels > u16::MAX as usize {
            return Err("waveform channel count is invalid".to_string());
        }
        Ok(Self {
            channels,
            current_frames: 0,
            current: vec![BucketStats::empty(); channels],
            buckets: Vec::new(),
            source_frames: 0,
        })
    }

    pub fn push_interleaved(&mut self, samples: &[f32]) -> Result<(), String> {
        if !samples.len().is_multiple_of(self.channels)
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err("waveform input is malformed".to_string());
        }
        for frame in samples.chunks_exact(self.channels) {
            for (channel, sample) in frame.iter().enumerate() {
                self.current[channel].add(*sample);
            }
            self.current_frames += 1;
            self.source_frames += 1;
            if self.current_frames == BASE_BUCKET_FRAMES {
                self.finish_bucket();
            }
        }
        Ok(())
    }

    pub fn finish(
        mut self,
        identity_digest: [u8; 32],
        sample_rate: u32,
    ) -> Result<WaveformCache, String> {
        if self.current_frames > 0 {
            self.finish_bucket();
        }
        if self.source_frames == 0 {
            return Err("waveform input is empty".to_string());
        }

        let mut levels = Vec::new();
        let mut buckets = self.buckets;
        let mut bucket_frames = BASE_BUCKET_FRAMES;
        loop {
            let mut values = Vec::with_capacity(buckets.len() * self.channels * 3);
            for bucket in &buckets {
                for channel in bucket {
                    values.extend_from_slice(&channel.values());
                }
            }
            levels.push(WaveformLevel {
                bucket_frames,
                values,
            });
            if buckets.len() <= 1 {
                break;
            }
            let mut coarser = Vec::with_capacity(buckets.len().div_ceil(LEVEL_SCALE as usize));
            for group in buckets.chunks(LEVEL_SCALE as usize) {
                let mut combined = vec![BucketStats::empty(); self.channels];
                for bucket in group {
                    for (target, source) in combined.iter_mut().zip(bucket) {
                        target.combine(*source);
                    }
                }
                coarser.push(combined);
            }
            buckets = coarser;
            bucket_frames = bucket_frames
                .checked_mul(LEVEL_SCALE)
                .ok_or_else(|| "waveform pyramid is too large".to_string())?;
        }

        Ok(WaveformCache {
            identity_digest,
            source_sample_rate: sample_rate,
            source_channels: self.channels as u16,
            source_frames: self.source_frames,
            levels,
        })
    }

    fn finish_bucket(&mut self) {
        self.buckets.push(std::mem::replace(
            &mut self.current,
            vec![BucketStats::empty(); self.channels],
        ));
        self.current_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pyramid_preserves_extrema_and_weighted_rms() {
        let mut builder = WaveformBuilder::new(1).unwrap();
        let mut samples = vec![0.5; BASE_BUCKET_FRAMES as usize];
        samples.extend(vec![-1.0; 44]);
        builder.push_interleaved(&samples).unwrap();
        let cache = builder.finish([1; 32], 48_000).unwrap();
        assert_eq!(cache.source_frames, 300);
        assert_eq!(cache.levels.len(), 2);
        assert_eq!(cache.levels[0].values[0..3], [0.5, 0.5, 0.5]);
        assert_eq!(cache.levels[0].values[3..6], [-1.0, -1.0, 1.0]);
        let overview = &cache.levels[1].values;
        assert_eq!(overview[0], -1.0);
        assert_eq!(overview[1], 0.5);
        let expected_rms = ((256.0 * 0.25 + 44.0) / 300.0_f32).sqrt();
        assert!((overview[2] - expected_rms).abs() < 0.000_001);
    }

    #[test]
    fn stereo_values_are_stored_per_channel() {
        let mut builder = WaveformBuilder::new(2).unwrap();
        builder.push_interleaved(&[-0.5, 0.25, 0.75, -1.0]).unwrap();
        let cache = builder.finish([2; 32], 44_100).unwrap();
        assert_eq!(
            cache.levels[0].values,
            vec![-0.5, 0.75, 0.637_377_44, -1.0, 0.25, 0.728_868_96]
        );
    }

    #[test]
    fn malformed_input_is_rejected() {
        let mut builder = WaveformBuilder::new(2).unwrap();
        assert!(builder.push_interleaved(&[0.0]).is_err());
        assert!(builder.push_interleaved(&[f32::NAN, 0.0]).is_err());
    }
}
