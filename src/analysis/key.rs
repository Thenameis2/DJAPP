use std::{cmp::Ordering, sync::Arc};

use rustfft::{num_complex::Complex, Fft, FftPlanner};

use super::{
    signal::ANALYSIS_SAMPLE_RATE,
    types::{Estimate, MusicalKey, MusicalMode},
};

const FFT_SIZE: usize = 8_192;
const HOP_SIZE: usize = 4_096;
const MIN_FREQUENCY: f64 = 55.0;
const MAX_FREQUENCY: f64 = 5_000.0;
const MAJOR_PROFILE: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR_PROFILE: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

#[derive(Clone, Debug, PartialEq)]
pub struct KeyAnalysis {
    pub key: Option<Estimate<MusicalKey>>,
    pub tuning_cents: Option<f32>,
    pub chroma: [f32; 12],
}

pub struct KeyAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buffer: Vec<Complex<f32>>,
    pending: Vec<f32>,
    pending_offset: usize,
    chroma_frames: Vec<[f64; 12]>,
    tuning_histogram: [f64; 101],
}

impl KeyAnalyzer {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        Self {
            fft: planner.plan_fft_forward(FFT_SIZE),
            window: (0..FFT_SIZE)
                .map(|index| {
                    0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (FFT_SIZE - 1) as f32).cos()
                })
                .collect(),
            buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            pending: Vec::new(),
            pending_offset: 0,
            chroma_frames: Vec::new(),
            tuning_histogram: [0.0; 101],
        }
    }

    pub fn analyze(&mut self, signal: &[f32]) -> Result<KeyAnalysis, String> {
        self.reset();
        self.push_signal(signal)?;
        self.finish()
    }

    pub fn push_signal(&mut self, signal: &[f32]) -> Result<(), String> {
        if signal.iter().any(|sample| !sample.is_finite()) {
            return Err("key signal contains non-finite samples".to_string());
        }
        if self.pending_offset > 0 {
            self.pending.drain(..self.pending_offset);
            self.pending_offset = 0;
        }
        self.pending.extend_from_slice(signal);
        while self.pending.len() - self.pending_offset >= FFT_SIZE {
            self.process_pending_frame();
            self.pending_offset += HOP_SIZE;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<KeyAnalysis, String> {
        let tuning_cents = tuning_estimate(&self.tuning_histogram);
        if self.chroma_frames.len() < 3 {
            return Ok(empty_analysis(tuning_cents));
        }
        let mut aggregate = [0.0_f64; 12];
        let mut accepted = 0_usize;
        for frame in &self.chroma_frames {
            let total = frame.iter().sum::<f64>();
            if total <= f64::EPSILON {
                continue;
            }
            let maximum = frame.iter().copied().fold(0.0_f64, f64::max);
            if maximum / total < 0.16 {
                continue;
            }
            for (target, value) in aggregate.iter_mut().zip(frame) {
                *target += value / total;
            }
            accepted += 1;
        }
        if accepted < 3 {
            return Ok(empty_analysis(tuning_cents));
        }
        let total = aggregate.iter().sum::<f64>();
        for value in &mut aggregate {
            *value /= total;
        }
        let maximum = aggregate.iter().copied().fold(0.0_f64, f64::max);
        let supported_pitch_classes = aggregate
            .iter()
            .filter(|value| **value >= maximum * 0.1)
            .count();
        if supported_pitch_classes < 3 {
            return Ok(KeyAnalysis {
                key: None,
                tuning_cents,
                chroma: aggregate.map(|value| value as f32),
            });
        }
        let uniformity = normalized_entropy(&aggregate);
        if uniformity > 0.96 {
            return Ok(KeyAnalysis {
                key: None,
                tuning_cents,
                chroma: aggregate.map(|value| value as f32),
            });
        }

        let mut candidates = Vec::with_capacity(24);
        for pitch_class in 0..12 {
            candidates.push((
                MusicalKey::new(pitch_class, MusicalMode::Major).expect("valid pitch class"),
                correlation(&aggregate, &rotate_profile(&MAJOR_PROFILE, pitch_class)),
            ));
            candidates.push((
                MusicalKey::new(pitch_class, MusicalMode::Minor).expect("valid pitch class"),
                correlation(&aggregate, &rotate_profile(&MINOR_PROFILE, pitch_class)),
            ));
        }
        candidates.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(Ordering::Equal));
        let best = candidates[0];
        let runner_up = candidates[1].1;
        let separation = ((best.1 - runner_up) / (1.0 - runner_up).max(0.05)).clamp(0.0, 1.0);
        let tonalness = (1.0 - uniformity).clamp(0.0, 1.0);
        let coverage = (accepted as f64 / self.chroma_frames.len() as f64).clamp(0.0, 1.0);
        let confidence = (0.45 * separation + 0.35 * tonalness + 0.20 * coverage) as f32;
        let key = (best.1 >= 0.45 && confidence >= 0.18)
            .then(|| Estimate::new(best.0, confidence.clamp(0.0, 1.0)))
            .flatten();
        Ok(KeyAnalysis {
            key,
            tuning_cents,
            chroma: aggregate.map(|value| value as f32),
        })
    }

    fn process_pending_frame(&mut self) {
        let samples = &self.pending[self.pending_offset..self.pending_offset + FFT_SIZE];
        let rms = (samples
            .iter()
            .map(|sample| f64::from(*sample).powi(2))
            .sum::<f64>()
            / FFT_SIZE as f64)
            .sqrt();
        if rms < 0.000_5 {
            return;
        }
        for (index, sample) in samples.iter().enumerate() {
            self.buffer[index] = Complex::new(*sample * self.window[index], 0.0);
        }
        self.fft.process(&mut self.buffer);
        let mut chroma = [0.0_f64; 12];
        let minimum_bin =
            (MIN_FREQUENCY * FFT_SIZE as f64 / f64::from(ANALYSIS_SAMPLE_RATE)) as usize;
        let maximum_bin =
            (MAX_FREQUENCY * FFT_SIZE as f64 / f64::from(ANALYSIS_SAMPLE_RATE)) as usize;
        for bin in minimum_bin.max(1)..maximum_bin.min(FFT_SIZE / 2 - 1) {
            let magnitude = f64::from(self.buffer[bin].norm());
            if magnitude <= f64::from(self.buffer[bin - 1].norm())
                || magnitude < f64::from(self.buffer[bin + 1].norm())
            {
                continue;
            }
            let frequency = bin as f64 * f64::from(ANALYSIS_SAMPLE_RATE) / FFT_SIZE as f64;
            let midi = 69.0 + 12.0 * (frequency / 440.0).log2();
            let nearest = midi.round();
            let cents = ((midi - nearest) * 100.0).round().clamp(-50.0, 50.0) as i32;
            let weight = magnitude.sqrt() / frequency.sqrt();
            self.tuning_histogram[(cents + 50) as usize] += weight;
            let pitch_class = (nearest as i32).rem_euclid(12) as usize;
            chroma[pitch_class] += weight;
        }
        if chroma.iter().sum::<f64>() > f64::EPSILON {
            self.chroma_frames.push(chroma);
        }
    }

    fn reset(&mut self) {
        self.pending.clear();
        self.pending_offset = 0;
        self.chroma_frames.clear();
        self.tuning_histogram.fill(0.0);
    }
}

impl Default for KeyAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

fn empty_analysis(tuning_cents: Option<f32>) -> KeyAnalysis {
    KeyAnalysis {
        key: None,
        tuning_cents,
        chroma: [0.0; 12],
    }
}

fn tuning_estimate(histogram: &[f64; 101]) -> Option<f32> {
    let total = histogram.iter().sum::<f64>();
    if total <= f64::EPSILON {
        return None;
    }
    let weighted = histogram
        .iter()
        .enumerate()
        .map(|(index, weight)| (index as f64 - 50.0) * weight)
        .sum::<f64>();
    Some((weighted / total) as f32)
}

fn rotate_profile(profile: &[f64; 12], root: u8) -> [f64; 12] {
    let mut rotated = [0.0; 12];
    for (interval, value) in profile.iter().enumerate() {
        rotated[(interval + usize::from(root)) % 12] = *value;
    }
    rotated
}

fn correlation(left: &[f64; 12], right: &[f64; 12]) -> f64 {
    let left_mean = left.iter().sum::<f64>() / 12.0;
    let right_mean = right.iter().sum::<f64>() / 12.0;
    let mut numerator = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (&left, &right) in left.iter().zip(right) {
        numerator += (left - left_mean) * (right - right_mean);
        left_energy += (left - left_mean).powi(2);
        right_energy += (right - right_mean).powi(2);
    }
    numerator / (left_energy * right_energy).sqrt().max(f64::EPSILON)
}

fn normalized_entropy(chroma: &[f64; 12]) -> f64 {
    let entropy = chroma
        .iter()
        .filter(|value| **value > f64::EPSILON)
        .map(|value| -value * value.ln())
        .sum::<f64>();
    entropy / 12.0_f64.ln()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::fixtures::{click_track, triad, TriadMode};

    #[test]
    fn detects_all_major_and_minor_triads() {
        for pitch_class in 0..12 {
            for (fixture_mode, expected_mode) in [
                (TriadMode::Major, MusicalMode::Major),
                (TriadMode::Minor, MusicalMode::Minor),
            ] {
                let signal = triad(pitch_class, fixture_mode, 0.0, 5);
                let result = KeyAnalyzer::new().analyze(&signal).unwrap();
                let estimate = result.key.unwrap_or_else(|| {
                    panic!("no key for pitch class {pitch_class} {expected_mode:?}")
                });
                assert_eq!(estimate.value.pitch_class, pitch_class);
                assert_eq!(estimate.value.mode, expected_mode);
            }
        }
    }

    #[test]
    fn tolerates_tuning_offsets() {
        for cents in [-35.0, -20.0, 20.0, 35.0] {
            let result = KeyAnalyzer::new()
                .analyze(&triad(9, TriadMode::Minor, cents, 5))
                .unwrap();
            assert_eq!(
                result.key.unwrap().value,
                MusicalKey::new(9, MusicalMode::Minor).unwrap()
            );
            assert!(result.tuning_cents.is_some());
        }
    }

    #[test]
    fn silence_and_noise_do_not_claim_a_key() {
        let silence = vec![0.0; ANALYSIS_SAMPLE_RATE as usize * 5];
        assert!(KeyAnalyzer::new().analyze(&silence).unwrap().key.is_none());
        let mut state = 1_u32;
        let noise: Vec<f32> = (0..ANALYSIS_SAMPLE_RATE as usize * 5)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state as f32 / u32::MAX as f32 - 0.5) * 0.5
            })
            .collect();
        assert!(KeyAnalyzer::new().analyze(&noise).unwrap().key.is_none());
        assert!(KeyAnalyzer::new()
            .analyze(&click_track(120.0, 8))
            .unwrap()
            .key
            .is_none());
    }
}
