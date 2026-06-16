use std::{cmp::Ordering, sync::Arc};

use rustfft::{num_complex::Complex, Fft, FftPlanner};

use super::{signal::ANALYSIS_SAMPLE_RATE, types::Estimate};

pub const FFT_SIZE: usize = 2_048;
pub const HOP_SIZE: usize = 512;
pub const MIN_BPM: f64 = 60.0;
pub const MAX_BPM: f64 = 200.0;
const SEGMENT_SECONDS: f64 = 20.0;
const SEGMENT_CANDIDATES: usize = 4;
const RHYTHM_BANDS: usize = 4;
const TEMPO_STATE_SECONDS: f64 = 12.0;

#[derive(Clone, Debug, PartialEq)]
pub struct TempoCandidate {
    pub bpm: f64,
    pub score: f64,
    pub grid: CandidateGridSupport,
}

impl TempoCandidate {
    pub fn new(bpm: f64, score: f64) -> Self {
        Self {
            bpm,
            score,
            grid: CandidateGridSupport::empty(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateGridSupport {
    pub score: f64,
    pub beat_strength: f64,
    pub offbeat_contrast: f64,
    pub stability: f64,
    pub section_consistency: f64,
    pub band_consensus: f64,
    pub tempo_state: f64,
    pub comb_filter: f64,
    pub beat_sequence: f64,
    pub octave_preference: f64,
    pub precision_adjustment_percent: f64,
    pub coverage: f64,
    pub octave_ambiguous: bool,
}

impl CandidateGridSupport {
    fn empty() -> Self {
        Self {
            score: 0.0,
            beat_strength: 0.0,
            offbeat_contrast: 0.0,
            stability: 0.0,
            section_consistency: 0.0,
            band_consensus: 0.0,
            tempo_state: 0.0,
            comb_filter: 0.0,
            beat_sequence: 0.0,
            octave_preference: 0.0,
            precision_adjustment_percent: 0.0,
            coverage: 0.0,
            octave_ambiguous: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RhythmAnalysis {
    pub bpm: Option<Estimate<f64>>,
    pub candidates: Vec<TempoCandidate>,
    pub onset_envelope: Vec<f32>,
    pub beat_grid: Option<TrackedBeatGrid>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackedBeat {
    pub analysis_frame: u64,
    pub strength: f32,
    pub downbeat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackedBeatGrid {
    pub confidence: f32,
    pub downbeat_confidence: Option<f32>,
    pub beats: Vec<TrackedBeat>,
}

pub struct RhythmAnalyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buffer: Vec<Complex<f32>>,
    previous_magnitudes: Vec<f32>,
    pending: Vec<f32>,
    pending_offset: usize,
    onset_envelope: Vec<f32>,
    band_envelopes: [Vec<f32>; RHYTHM_BANDS],
    signal_frames: u64,
}

impl RhythmAnalyzer {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let window = (0..FFT_SIZE)
            .map(|index| {
                0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / (FFT_SIZE - 1) as f32).cos()
            })
            .collect();
        Self {
            fft,
            window,
            buffer: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            previous_magnitudes: vec![0.0; FFT_SIZE / 2 + 1],
            pending: Vec::new(),
            pending_offset: 0,
            onset_envelope: Vec::new(),
            band_envelopes: std::array::from_fn(|_| Vec::new()),
            signal_frames: 0,
        }
    }

    pub fn analyze(&mut self, signal: &[f32]) -> Result<RhythmAnalysis, String> {
        self.reset();
        self.push_signal(signal)?;
        self.finish()
    }

    pub fn push_signal(&mut self, signal: &[f32]) -> Result<(), String> {
        if signal.iter().any(|sample| !sample.is_finite()) {
            return Err("rhythm signal contains non-finite samples".to_string());
        }
        self.signal_frames = self
            .signal_frames
            .checked_add(signal.len() as u64)
            .ok_or_else(|| "rhythm signal is too long".to_string())?;
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

    pub fn finish(&mut self) -> Result<RhythmAnalysis, String> {
        if self.onset_envelope.is_empty() {
            return Ok(RhythmAnalysis {
                bpm: None,
                candidates: Vec::new(),
                onset_envelope: Vec::new(),
                beat_grid: None,
            });
        }
        let mut envelope = std::mem::take(&mut self.onset_envelope);
        let mut band_envelopes = std::mem::take(&mut self.band_envelopes);
        normalize_envelope(&mut envelope);
        for band in &mut band_envelopes {
            normalize_envelope(band);
        }
        if envelope.iter().filter(|value| **value >= 0.15).count() < 4 {
            return Ok(RhythmAnalysis {
                bpm: None,
                candidates: Vec::new(),
                onset_envelope: envelope,
                beat_grid: None,
            });
        }
        let candidates = tempo_candidates(&envelope, &band_envelopes);
        let bpm = candidates
            .first()
            .and_then(|best| Estimate::new(best.bpm, tempo_confidence_for_candidates(&candidates)));
        let beat_grid = bpm.and_then(|estimate| {
            track_beats(
                &envelope,
                estimate.value,
                estimate.confidence,
                self.signal_frames,
            )
        });
        Ok(RhythmAnalysis {
            bpm,
            candidates,
            onset_envelope: envelope,
            beat_grid,
        })
    }

    fn reset(&mut self) {
        self.previous_magnitudes.fill(0.0);
        self.pending.clear();
        self.pending_offset = 0;
        self.onset_envelope.clear();
        for band in &mut self.band_envelopes {
            band.clear();
        }
        self.signal_frames = 0;
    }

    fn process_pending_frame(&mut self) {
        for index in 0..FFT_SIZE {
            self.buffer[index] = Complex::new(
                self.pending[self.pending_offset + index] * self.window[index],
                0.0,
            );
        }
        self.fft.process(&mut self.buffer);
        let mut flux = 0.0_f32;
        let mut band_fluxes = [0.0_f32; RHYTHM_BANDS];
        for (index, previous) in self.previous_magnitudes.iter_mut().enumerate() {
            let magnitude = self.buffer[index].norm();
            let positive_flux = (magnitude - *previous).max(0.0);
            let band = spectral_flux_band(index);
            if let Some(band) = band {
                band_fluxes[band] += positive_flux;
            }
            flux += positive_flux * spectral_flux_weight(index);
            *previous = magnitude;
        }
        self.onset_envelope.push(flux);
        for (band, flux) in self.band_envelopes.iter_mut().zip(band_fluxes) {
            band.push(flux);
        }
    }
}

fn spectral_flux_band(bin: usize) -> Option<usize> {
    let frequency = bin as f32 * ANALYSIS_SAMPLE_RATE as f32 / FFT_SIZE as f32;
    if (45.0..=180.0).contains(&frequency) {
        Some(0)
    } else if (180.0..=360.0).contains(&frequency) {
        Some(1)
    } else if (360.0..=2_000.0).contains(&frequency) {
        Some(2)
    } else if (2_000.0..=8_000.0).contains(&frequency) {
        Some(3)
    } else {
        None
    }
}

fn spectral_flux_weight(bin: usize) -> f32 {
    let frequency = bin as f32 * ANALYSIS_SAMPLE_RATE as f32 / FFT_SIZE as f32;
    if (45.0..=180.0).contains(&frequency) {
        2.5
    } else if (180.0..=360.0).contains(&frequency) {
        1.6
    } else if frequency <= 4_000.0 {
        1.0
    } else {
        0.75
    }
}

fn tempo_confidence(best_score: f64, runner_up_score: f64) -> f32 {
    let separation = ((best_score - runner_up_score) / best_score.max(f64::EPSILON)).max(0.0);
    let evidence = (best_score / 0.65).clamp(0.0, 1.0);
    let confidence = 0.20 + 0.45 * evidence + 0.35 * separation;
    let ambiguity_cap = if separation < 0.35 {
        0.45 + 0.70 * separation
    } else {
        1.0
    };
    confidence.min(ambiguity_cap).clamp(0.0, 1.0) as f32
}

fn tempo_confidence_for_candidates(candidates: &[TempoCandidate]) -> f32 {
    let Some(best) = candidates.first() else {
        return 0.0;
    };
    let runner_up = candidates.get(1).map_or(0.0, |candidate| candidate.score);
    let mut confidence = tempo_confidence(best.score, runner_up);
    if best.grid.octave_ambiguous {
        confidence = confidence.min(0.58);
    }
    confidence
}

fn track_beats(
    envelope: &[f32],
    bpm: f64,
    bpm_confidence: f32,
    signal_frames: u64,
) -> Option<TrackedBeatGrid> {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let expected_period = 60.0 * envelope_rate / bpm;
    let peaks = onset_peaks(envelope);
    if peaks.len() < 4 || signal_frames == 0 {
        return None;
    }

    let mut scores = vec![f64::NEG_INFINITY; peaks.len()];
    let mut predecessors = vec![None; peaks.len()];
    for (index, &(position, strength)) in peaks.iter().enumerate() {
        scores[index] = f64::from(strength);
        for previous in (0..index).rev() {
            let interval = (position - peaks[previous].0) as f64;
            if interval > expected_period * 2.2 {
                break;
            }
            if interval < expected_period * 0.45 {
                continue;
            }
            let periods = (interval / expected_period).round().clamp(1.0, 2.0);
            let ratio = interval / (expected_period * periods);
            let penalty = 2.5 * ratio.log2().powi(2) + 0.08 * (periods - 1.0);
            let candidate = scores[previous] + f64::from(strength) - penalty;
            if candidate > scores[index] {
                scores[index] = candidate;
                predecessors[index] = Some(previous);
            }
        }
    }

    let terminal = scores
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.partial_cmp(right.1).unwrap_or(Ordering::Equal))?
        .0;
    let mut chain = Vec::new();
    let mut cursor = Some(terminal);
    while let Some(index) = cursor {
        chain.push(peaks[index]);
        cursor = predecessors[index];
    }
    chain.reverse();
    if chain.len() < 4 {
        return None;
    }

    let (origin, period) = fit_beat_line(&chain, expected_period)?;
    let first_number = ((-origin) / period).ceil() as i64;
    let last_number = ((signal_frames as f64 - 1.0 - origin) / period).floor() as i64;
    if last_number < first_number {
        return None;
    }
    let mut beats = Vec::with_capacity((last_number - first_number + 1) as usize);
    for number in first_number..=last_number {
        let analysis_frame = (origin + number as f64 * period).round().max(0.0) as u64;
        if analysis_frame >= signal_frames {
            continue;
        }
        let envelope_position =
            analysis_frame.saturating_sub((FFT_SIZE / 2) as u64) as f64 / HOP_SIZE as f64;
        let strength = sample_envelope(envelope, envelope_position);
        beats.push(TrackedBeat {
            analysis_frame,
            strength,
            downbeat: false,
        });
    }
    if beats.len() < 4 {
        return None;
    }

    let chain_coverage = (chain.len() as f32 / beats.len() as f32).clamp(0.0, 1.0);
    let mean_strength = beats.iter().map(|beat| beat.strength).sum::<f32>() / beats.len() as f32;
    let confidence =
        (0.55 * bpm_confidence + 0.25 * chain_coverage + 0.20 * mean_strength.clamp(0.0, 1.0))
            .clamp(0.0, 1.0);
    let downbeat_confidence = mark_downbeats(&mut beats);
    Some(TrackedBeatGrid {
        confidence,
        downbeat_confidence,
        beats,
    })
}

fn onset_peaks(envelope: &[f32]) -> Vec<(usize, f32)> {
    (1..envelope.len().saturating_sub(1))
        .filter_map(|index| {
            let strength = envelope[index];
            (strength >= 0.1 && strength >= envelope[index - 1] && strength >= envelope[index + 1])
                .then_some((index, strength))
        })
        .collect()
}

fn fit_beat_line(chain: &[(usize, f32)], expected_period: f64) -> Option<(f64, f64)> {
    let mut beat_numbers = Vec::with_capacity(chain.len());
    beat_numbers.push(0.0_f64);
    for pair in chain.windows(2) {
        let count = ((pair[1].0 - pair[0].0) as f64 / expected_period)
            .round()
            .clamp(1.0, 2.0);
        beat_numbers.push(beat_numbers.last().copied()? + count);
    }
    let mean_number = beat_numbers.iter().sum::<f64>() / beat_numbers.len() as f64;
    let positions: Vec<f64> = chain
        .iter()
        .map(|(position, _)| position * HOP_SIZE + FFT_SIZE / 2)
        .map(|position| position as f64)
        .collect();
    let mean_position = positions.iter().sum::<f64>() / positions.len() as f64;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (&number, &position) in beat_numbers.iter().zip(&positions) {
        covariance += (number - mean_number) * (position - mean_position);
        variance += (number - mean_number).powi(2);
    }
    let period = covariance / variance;
    let expected_samples = expected_period * HOP_SIZE as f64;
    if !period.is_finite() || !(expected_samples * 0.85..=expected_samples * 1.15).contains(&period)
    {
        return None;
    }
    Some((mean_position - period * mean_number, period))
}

fn sample_envelope(envelope: &[f32], position: f64) -> f32 {
    if envelope.is_empty() {
        return 0.0;
    }
    let center = position.round().max(0.0) as usize;
    let start = center.saturating_sub(1);
    if start >= envelope.len() {
        return 0.0;
    }
    let end = center.saturating_add(2).min(envelope.len());
    envelope[start..end].iter().copied().fold(0.0, f32::max)
}

fn mark_downbeats(beats: &mut [TrackedBeat]) -> Option<f32> {
    if beats.len() < 12 {
        return None;
    }
    let mut phase_scores = [0.0_f32; 4];
    let mut phase_counts = [0_usize; 4];
    for (index, beat) in beats.iter().enumerate() {
        phase_scores[index % 4] += beat.strength;
        phase_counts[index % 4] += 1;
    }
    for phase in 0..4 {
        phase_scores[phase] /= phase_counts[phase].max(1) as f32;
    }
    let mut ranked = [(0_usize, 0.0_f32); 4];
    for (phase, score) in phase_scores.into_iter().enumerate() {
        ranked[phase] = (phase, score);
    }
    ranked.sort_by(|left, right| right.1.partial_cmp(&left.1).unwrap_or(Ordering::Equal));
    let confidence = ((ranked[0].1 - ranked[1].1) / ranked[0].1.max(f32::EPSILON)).clamp(0.0, 1.0);
    if confidence < 0.2 || ranked[0].1 < 0.2 {
        return None;
    }
    for (index, beat) in beats.iter_mut().enumerate() {
        beat.downbeat = index % 4 == ranked[0].0;
    }
    Some(confidence)
}

impl Default for RhythmAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

fn normalize_envelope(envelope: &mut [f32]) {
    if envelope.is_empty() {
        return;
    }
    let radius = 16;
    let mut prefix = Vec::with_capacity(envelope.len() + 1);
    prefix.push(0.0_f64);
    for value in envelope.iter() {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + f64::from(*value));
    }
    for (index, value) in envelope.iter_mut().enumerate() {
        let start = index.saturating_sub(radius);
        let end = (index + radius + 1).min(prefix.len() - 1);
        let local_mean = ((prefix[end] - prefix[start]) / (end - start) as f64) as f32;
        *value = (*value - local_mean).max(0.0);
    }
    let maximum = envelope.iter().copied().fold(0.0_f32, f32::max);
    if maximum > 0.0 {
        for value in envelope {
            *value /= maximum;
        }
    }
}

fn tempo_candidates(
    envelope: &[f32],
    band_envelopes: &[Vec<f32>; RHYTHM_BANDS],
) -> Vec<TempoCandidate> {
    let mut candidates = correlation_tempo_candidates(envelope);
    candidates.extend(segment_consensus_candidates(envelope));
    candidates.extend(band_consensus_candidates(band_envelopes));
    candidates.extend(comb_filter_tempo_candidates(envelope));
    let candidates = merge_close_candidates(candidates, 0.015);
    rerank_with_beat_support(envelope, band_envelopes, candidates)
}

fn correlation_tempo_candidates(envelope: &[f32]) -> Vec<TempoCandidate> {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let minimum_lag = (60.0 * envelope_rate / MAX_BPM).ceil() as usize;
    let maximum_lag = (60.0 * envelope_rate / MIN_BPM).floor() as usize;
    let mut correlations = vec![0.0; maximum_lag + 1];
    let last_lag = maximum_lag.min(envelope.len().saturating_sub(2));
    for (lag, correlation) in correlations
        .iter_mut()
        .enumerate()
        .take(last_lag + 1)
        .skip(minimum_lag)
    {
        *correlation = normalized_correlation(envelope, lag);
    }
    let mut candidates = Vec::new();
    for lag in minimum_lag..=last_lag {
        let base = correlations[lag];
        if base <= 0.0
            || (lag > minimum_lag && correlations[lag - 1] > base)
            || (lag < maximum_lag && correlations.get(lag + 1).copied().unwrap_or(0.0) > base)
        {
            continue;
        }
        let harmonic = neighborhood_max(&correlations, lag * 2, 2);
        let subharmonic = neighborhood_max(&correlations, lag / 2, 1);
        let refined_lag = refine_peak(&correlations, lag);
        let unrefined_bpm = 60.0 * envelope_rate / refined_lag;
        let bpm = refine_tempo_from_peaks(envelope, refined_lag)
            .map(|period| 60.0 * envelope_rate / period)
            .filter(|bpm| (MIN_BPM..=MAX_BPM).contains(bpm))
            .unwrap_or(unrefined_bpm);
        let center_weight = (-((bpm / 120.0).log2() / 1.6).powi(2)).exp();
        let score = base + 0.6 * harmonic + 0.1 * subharmonic + 0.2 * center_weight;
        candidates.push(TempoCandidate::new(bpm, score));
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    candidates.truncate(8);
    candidates
}

fn segment_consensus_candidates(envelope: &[f32]) -> Vec<TempoCandidate> {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let segment_len = (SEGMENT_SECONDS * envelope_rate).round() as usize;
    if envelope.len() < segment_len + segment_len / 2 {
        return Vec::new();
    }
    let hop = (segment_len / 2).max(1);
    let mut clusters = Vec::<TempoCluster>::new();
    let mut windows = 0_usize;
    let mut start = 0_usize;
    while start < envelope.len() {
        let end = (start + segment_len).min(envelope.len());
        if end - start < segment_len / 2 {
            break;
        }
        let mut segment = envelope[start..end].to_vec();
        normalize_envelope(&mut segment);
        let local = correlation_tempo_candidates(&segment);
        if !local.is_empty() {
            windows += 1;
        }
        for (rank, candidate) in local.iter().take(SEGMENT_CANDIDATES).enumerate() {
            let rank_weight = (1.0 - rank as f64 * 0.15).max(0.55);
            for (bpm, variant_weight) in harmonic_variants(candidate.bpm) {
                add_tempo_cluster(
                    &mut clusters,
                    bpm,
                    candidate.score * rank_weight * variant_weight,
                    0.025,
                );
            }
        }
        start += hop;
    }
    if windows < 2 {
        return Vec::new();
    }
    let mut candidates: Vec<_> = clusters
        .into_iter()
        .filter(|cluster| cluster.hits >= 2)
        .map(|cluster| {
            TempoCandidate::new(
                cluster.bpm,
                cluster.score / windows as f64
                    + 0.45 * (cluster.hits as f64 / windows as f64).clamp(0.0, 1.0),
            )
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    candidates.truncate(8);
    candidates
}

fn band_consensus_candidates(band_envelopes: &[Vec<f32>; RHYTHM_BANDS]) -> Vec<TempoCandidate> {
    let mut clusters = Vec::<TempoCluster>::new();
    let mut active_bands = 0_usize;
    for (band_index, envelope) in band_envelopes.iter().enumerate() {
        if envelope.iter().filter(|value| **value >= 0.15).count() < 4 {
            continue;
        }
        let candidates = correlation_tempo_candidates(envelope);
        if candidates.is_empty() {
            continue;
        }
        active_bands += 1;
        let band_weight = match band_index {
            0 => 1.15,
            1 => 1.05,
            2 => 0.85,
            _ => 0.70,
        };
        for (rank, candidate) in candidates.iter().take(SEGMENT_CANDIDATES).enumerate() {
            let rank_weight = (1.0 - rank as f64 * 0.18).max(0.45);
            for (bpm, variant_weight) in harmonic_variants(candidate.bpm) {
                add_tempo_cluster(
                    &mut clusters,
                    bpm,
                    candidate.score * band_weight * rank_weight * variant_weight,
                    0.025,
                );
            }
        }
    }
    if active_bands < 2 {
        return Vec::new();
    }
    let mut candidates: Vec<_> = clusters
        .into_iter()
        .filter(|cluster| cluster.hits >= 2)
        .map(|cluster| {
            TempoCandidate::new(
                cluster.bpm,
                cluster.score / active_bands as f64
                    + 0.35 * (cluster.hits as f64 / active_bands as f64).clamp(0.0, 1.0),
            )
        })
        .collect();
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    candidates.truncate(8);
    candidates
}

fn comb_filter_tempo_candidates(envelope: &[f32]) -> Vec<TempoCandidate> {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let minimum_lag = (60.0 * envelope_rate / MAX_BPM).ceil() as usize;
    let maximum_lag = (60.0 * envelope_rate / MIN_BPM).floor() as usize;
    let mut candidates = Vec::new();
    for lag in minimum_lag..=maximum_lag.min(envelope.len().saturating_sub(2)) {
        let score = comb_filter_lag_score(envelope, lag as f64);
        if score <= 0.0 {
            continue;
        }
        let left = if lag > minimum_lag {
            comb_filter_lag_score(envelope, lag as f64 - 1.0)
        } else {
            0.0
        };
        let right = if lag < maximum_lag {
            comb_filter_lag_score(envelope, lag as f64 + 1.0)
        } else {
            0.0
        };
        if left > score || right > score {
            continue;
        }
        let bpm = 60.0 * envelope_rate / lag as f64;
        candidates.push(TempoCandidate::new(bpm, score));
    }
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    candidates.truncate(8);
    candidates
}

#[derive(Clone, Copy, Debug)]
struct TempoCluster {
    bpm: f64,
    score: f64,
    hits: usize,
}

fn harmonic_variants(bpm: f64) -> Vec<(f64, f64)> {
    let mut variants = vec![(bpm, 1.0)];
    let half = bpm / 2.0;
    if half >= MIN_BPM {
        variants.push((half, 0.82));
    }
    let double = bpm * 2.0;
    if double <= MAX_BPM {
        variants.push((double, 0.82));
    }
    variants
}

fn add_tempo_cluster(clusters: &mut Vec<TempoCluster>, bpm: f64, score: f64, tolerance: f64) {
    if !(MIN_BPM..=MAX_BPM).contains(&bpm) || !score.is_finite() || score <= 0.0 {
        return;
    }
    if let Some(cluster) = clusters
        .iter_mut()
        .find(|cluster| relative_bpm_error(cluster.bpm, bpm) <= tolerance)
    {
        let previous_score = cluster.score;
        cluster.score += score;
        cluster.bpm = (cluster.bpm * previous_score + bpm * score) / cluster.score;
        cluster.hits += 1;
    } else {
        clusters.push(TempoCluster {
            bpm,
            score,
            hits: 1,
        });
    }
}

fn merge_close_candidates(
    mut candidates: Vec<TempoCandidate>,
    tolerance: f64,
) -> Vec<TempoCandidate> {
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    let mut merged = Vec::<TempoCandidate>::new();
    for candidate in candidates {
        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| relative_bpm_error(existing.bpm, candidate.bpm) <= tolerance)
        {
            if candidate.score > existing.score {
                *existing = candidate;
            }
        } else {
            merged.push(candidate);
        }
    }
    merged.truncate(8);
    merged
}

fn rerank_with_beat_support(
    envelope: &[f32],
    band_envelopes: &[Vec<f32>; RHYTHM_BANDS],
    candidates: Vec<TempoCandidate>,
) -> Vec<TempoCandidate> {
    if candidates.len() <= 1 {
        return candidates;
    }
    let max_score = candidates
        .iter()
        .map(|candidate| candidate.score)
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);
    let mut rescored: Vec<_> = candidates
        .into_iter()
        .map(|mut candidate| {
            let original_bpm = candidate.bpm;
            let (bpm, mut grid) =
                refine_candidate_precision(envelope, band_envelopes, original_bpm);
            grid.precision_adjustment_percent = ((bpm - original_bpm) / original_bpm) * 100.0;
            let source_score = candidate.score / max_score;
            candidate.grid = grid;
            TempoCandidate {
                bpm,
                score: 0.35 * source_score + 0.65 * grid.score,
                grid,
            }
        })
        .collect();
    resolve_tempo_octaves(&mut rescored);
    mark_octave_ambiguity(&mut rescored);
    rescored.truncate(8);
    rescored
}

fn refine_candidate_precision(
    envelope: &[f32],
    band_envelopes: &[Vec<f32>; RHYTHM_BANDS],
    bpm: f64,
) -> (f64, CandidateGridSupport) {
    let base_grid = single_envelope_grid_support(envelope, bpm);
    if base_grid.score <= 0.0 {
        return (bpm, candidate_grid_support(envelope, band_envelopes, bpm));
    }
    let mut best_bpm = bpm;
    let mut best_score = base_grid.score;
    for step in -10..=10 {
        if step == 0 {
            continue;
        }
        let adjusted_bpm = bpm * (1.0 + step as f64 * 0.0035);
        if !(MIN_BPM..=MAX_BPM).contains(&adjusted_bpm) {
            continue;
        }
        let grid = single_envelope_grid_support(envelope, adjusted_bpm);
        if grid.score > best_score + 0.002 {
            best_bpm = adjusted_bpm;
            best_score = grid.score;
        }
    }
    (
        best_bpm,
        candidate_grid_support(envelope, band_envelopes, best_bpm),
    )
}

fn resolve_tempo_octaves(candidates: &mut [TempoCandidate]) {
    if candidates.len() <= 1 {
        return;
    }
    let snapshot = candidates.to_vec();
    for candidate in candidates.iter_mut() {
        let mut preference = octave_preference_score(candidate);
        if let Some(half) = related_candidate(&snapshot, candidate.bpm / 2.0) {
            if candidate.grid.tempo_state > half.grid.tempo_state + 0.04
                && candidate.score >= half.score * 0.92
            {
                preference += 0.04;
            }
        }
        if let Some(double) = related_candidate(&snapshot, candidate.bpm * 2.0) {
            if double.grid.tempo_state > candidate.grid.tempo_state + 0.04
                && double.score >= candidate.score * 0.92
            {
                preference -= 0.04;
            }
        }
        candidate.grid.octave_preference = preference.clamp(0.0, 1.0);
        candidate.score = candidate.grid.octave_preference;
    }
    apply_precision_tie_breakers(candidates);
    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
}

fn apply_precision_tie_breakers(candidates: &mut [TempoCandidate]) {
    let snapshot = candidates.to_vec();
    for candidate in candidates {
        let stronger_close_grid = snapshot.iter().any(|other| {
            !same_bpm(candidate.bpm, other.bpm)
                && other.score >= candidate.score
                && candidate.score >= other.score - 0.012
                && candidate.grid.score > other.grid.score + 0.035
                && candidate.grid.tempo_state > other.grid.tempo_state + 0.025
                && candidate.grid.beat_strength > other.grid.beat_strength + 0.035
        });
        if stronger_close_grid {
            candidate.score += 0.018;
            candidate.grid.octave_preference = candidate.score;
        }
    }
}

fn same_bpm(left: f64, right: f64) -> bool {
    relative_bpm_error(left, right) <= 0.005
}

fn octave_preference_score(candidate: &TempoCandidate) -> f64 {
    (0.58 * candidate.score
        + 0.22 * candidate.grid.tempo_state
        + 0.12 * candidate.grid.section_consistency
        + 0.08 * candidate.grid.band_consensus)
        .clamp(0.0, 1.0)
}

fn related_candidate(candidates: &[TempoCandidate], bpm: f64) -> Option<&TempoCandidate> {
    candidates
        .iter()
        .filter(|candidate| relative_bpm_error(candidate.bpm, bpm) <= 0.03)
        .max_by(|left, right| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(Ordering::Equal)
        })
}

fn mark_octave_ambiguity(candidates: &mut [TempoCandidate]) {
    if candidates.len() < 2 {
        return;
    }
    let snapshot = candidates.to_vec();
    for candidate in candidates {
        candidate.grid.octave_ambiguous = snapshot.iter().any(|other| {
            let same_candidate = relative_bpm_error(candidate.bpm, other.bpm) <= 0.005;
            let half_or_double = relative_bpm_error(candidate.bpm * 2.0, other.bpm) <= 0.03
                || relative_bpm_error(candidate.bpm / 2.0, other.bpm) <= 0.03;
            !same_candidate && half_or_double && other.score >= candidate.score * 0.90
        });
    }
}

fn candidate_grid_support(
    envelope: &[f32],
    band_envelopes: &[Vec<f32>; RHYTHM_BANDS],
    bpm: f64,
) -> CandidateGridSupport {
    let mut support = single_envelope_grid_support(envelope, bpm);
    if support.score <= 0.0 {
        return support;
    }
    let mut band_scores = Vec::new();
    for envelope in band_envelopes {
        if envelope.iter().filter(|value| **value >= 0.15).count() < 4 {
            continue;
        }
        let band_support = single_envelope_grid_support(envelope, bpm);
        if band_support.score > 0.0 {
            band_scores.push(band_support.score);
        }
    }
    support.band_consensus = band_consensus_score(&band_scores);
    support.tempo_state = tempo_state_support(envelope, bpm);
    support.comb_filter = comb_filter_support(envelope, bpm);
    support.beat_sequence = beat_sequence_support(envelope, bpm);
    support.score = (0.36 * support.score
        + 0.14 * support.band_consensus
        + 0.18 * support.tempo_state
        + 0.12 * support.comb_filter
        + 0.20 * support.beat_sequence)
        .clamp(0.0, 1.0);
    support
}

fn single_envelope_grid_support(envelope: &[f32], bpm: f64) -> CandidateGridSupport {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let period = 60.0 * envelope_rate / bpm;
    if !period.is_finite() || period < 1.0 || envelope.len() < (period * 4.0) as usize {
        return CandidateGridSupport::empty();
    }
    let phase_steps = period.round().clamp(4.0, 96.0) as usize;
    let mut best = CandidateGridSupport::empty();
    for phase_step in 0..phase_steps {
        let phase = phase_step as f64 * period / phase_steps as f64;
        let mut beat_strengths = Vec::new();
        let mut beat_positions = Vec::new();
        let mut offbeat_sum = 0.0_f64;
        let mut offbeat_count = 0_usize;
        let mut position = phase;
        while position < envelope.len() as f64 {
            beat_strengths.push(f64::from(sample_envelope(envelope, position)));
            beat_positions.push(position);
            let offbeat = position + period * 0.5;
            if offbeat < envelope.len() as f64 {
                offbeat_sum += f64::from(sample_envelope(envelope, offbeat));
                offbeat_count += 1;
            }
            position += period;
        }
        if beat_strengths.len() < 4 {
            continue;
        }
        let mean = beat_strengths.iter().sum::<f64>() / beat_strengths.len() as f64;
        let variance = beat_strengths
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / beat_strengths.len() as f64;
        let stability = 1.0 / (1.0 + variance.sqrt());
        let offbeat_mean = if offbeat_count == 0 {
            0.0
        } else {
            offbeat_sum / offbeat_count as f64
        };
        let contrast = ((mean - 0.45 * offbeat_mean) / mean.max(f64::EPSILON)).clamp(0.0, 1.0);
        let coverage = (beat_strengths.len() as f64 / 24.0).clamp(0.0, 1.0);
        let section_consistency =
            section_consistency(envelope.len(), &beat_positions, &beat_strengths);
        let score = (0.38 * mean + 0.24 * contrast + 0.18 * stability + 0.20 * section_consistency)
            * coverage;
        if score > best.score {
            best = CandidateGridSupport {
                score,
                beat_strength: mean.clamp(0.0, 1.0),
                offbeat_contrast: contrast,
                stability,
                section_consistency,
                band_consensus: 0.0,
                tempo_state: 0.0,
                comb_filter: 0.0,
                beat_sequence: 0.0,
                octave_preference: 0.0,
                precision_adjustment_percent: 0.0,
                coverage,
                octave_ambiguous: false,
            };
        }
    }
    best.score = best.score.clamp(0.0, 1.0);
    best
}

fn band_consensus_score(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let active = scores.iter().filter(|score| **score >= 0.35).count() as f64 / RHYTHM_BANDS as f64;
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    let variance = scores
        .iter()
        .map(|score| (score - mean).powi(2))
        .sum::<f64>()
        / scores.len() as f64;
    let stability = 1.0 / (1.0 + variance.sqrt());
    (0.55 * mean + 0.25 * active + 0.20 * stability).clamp(0.0, 1.0)
}

fn tempo_state_support(envelope: &[f32], bpm: f64) -> f64 {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let segment_len = (TEMPO_STATE_SECONDS * envelope_rate).round() as usize;
    if envelope.len() < segment_len + segment_len / 2 {
        return 0.0;
    }
    let hop = (segment_len / 2).max(1);
    let mut window_scores = Vec::new();
    let mut start = 0_usize;
    while start < envelope.len() {
        let end = (start + segment_len).min(envelope.len());
        if end - start < segment_len / 2 {
            break;
        }
        let mut segment = envelope[start..end].to_vec();
        normalize_envelope(&mut segment);
        let local_candidates = correlation_tempo_candidates(&segment);
        let candidate_score = temporal_candidate_score(bpm, &local_candidates);
        let grid_score = single_envelope_grid_support(&segment, bpm).score;
        window_scores.push((0.65 * candidate_score + 0.35 * grid_score).clamp(0.0, 1.0));
        start += hop;
    }
    if window_scores.len() < 2 {
        return 0.0;
    }
    let active = window_scores.iter().filter(|score| **score >= 0.38).count() as f64
        / window_scores.len() as f64;
    let mean = window_scores.iter().sum::<f64>() / window_scores.len() as f64;
    let variance = window_scores
        .iter()
        .map(|score| (score - mean).powi(2))
        .sum::<f64>()
        / window_scores.len() as f64;
    let stability = 1.0 / (1.0 + variance.sqrt());
    (0.55 * mean + 0.30 * active + 0.15 * stability).clamp(0.0, 1.0)
}

fn temporal_candidate_score(bpm: f64, candidates: &[TempoCandidate]) -> f64 {
    let best_score = candidates
        .iter()
        .map(|candidate| candidate.score)
        .fold(0.0_f64, f64::max)
        .max(f64::EPSILON);
    candidates
        .iter()
        .enumerate()
        .map(|(rank, candidate)| {
            let rank_weight = (1.0 - rank as f64 * 0.12).max(0.40);
            let normalized = candidate.score / best_score;
            let relation_weight = tempo_relation_weight(bpm, candidate.bpm);
            normalized * rank_weight * relation_weight
        })
        .fold(0.0, f64::max)
        .clamp(0.0, 1.0)
}

fn tempo_relation_weight(target_bpm: f64, candidate_bpm: f64) -> f64 {
    if relative_bpm_error(target_bpm, candidate_bpm) <= 0.025 {
        1.0
    } else if relative_bpm_error(target_bpm * 2.0, candidate_bpm) <= 0.025
        || relative_bpm_error(target_bpm / 2.0, candidate_bpm) <= 0.025
    {
        0.62
    } else {
        0.0
    }
}

fn comb_filter_support(envelope: &[f32], bpm: f64) -> f64 {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let lag = 60.0 * envelope_rate / bpm;
    if !lag.is_finite() || lag < 1.0 {
        return 0.0;
    }
    comb_filter_lag_score(envelope, lag)
}

fn comb_filter_lag_score(envelope: &[f32], lag: f64) -> f64 {
    if envelope.is_empty() || !lag.is_finite() || lag < 1.0 {
        return 0.0;
    }
    let harmonic_weights = [(1.0_f64, 1.0_f64), (2.0, 0.72), (3.0, 0.48), (4.0, 0.32)];
    let mut best = 0.0_f64;
    let phase_steps = lag.round().clamp(4.0, 96.0) as usize;
    for phase_step in 0..phase_steps {
        let phase = phase_step as f64 * lag / phase_steps as f64;
        let mut total = 0.0_f64;
        let mut weight_total = 0.0_f64;
        let mut expected_beats = 0_usize;
        let mut position = phase;
        while position < envelope.len() as f64 {
            expected_beats += 1;
            for (multiple, weight) in harmonic_weights {
                let harmonic_position = position + lag * (multiple - 1.0);
                if harmonic_position >= envelope.len() as f64 {
                    continue;
                }
                total += f64::from(sample_envelope(envelope, harmonic_position)) * weight;
                weight_total += weight;
            }
            position += lag;
        }
        if expected_beats < 4 || weight_total <= f64::EPSILON {
            continue;
        }
        let mean = total / weight_total;
        let coverage = (expected_beats as f64 / 24.0).clamp(0.0, 1.0);
        best = best.max(mean * coverage);
    }
    best.clamp(0.0, 1.0)
}

fn beat_sequence_support(envelope: &[f32], bpm: f64) -> f64 {
    let envelope_rate = f64::from(ANALYSIS_SAMPLE_RATE) / HOP_SIZE as f64;
    let expected_period = 60.0 * envelope_rate / bpm;
    if !expected_period.is_finite() || expected_period < 1.0 {
        return 0.0;
    }
    let peaks = onset_peaks(envelope);
    if peaks.len() < 4 {
        return 0.0;
    }

    let mut scores = vec![f64::NEG_INFINITY; peaks.len()];
    let mut predecessors = vec![None; peaks.len()];
    let mut chain_lengths = vec![1_usize; peaks.len()];
    for (index, &(position, strength)) in peaks.iter().enumerate() {
        scores[index] = f64::from(strength);
        for previous in (0..index).rev() {
            let interval = (position - peaks[previous].0) as f64;
            if interval > expected_period * 2.35 {
                break;
            }
            if interval < expected_period * 0.42 {
                continue;
            }
            let periods = (interval / expected_period).round().clamp(1.0, 2.0);
            let ratio = interval / (expected_period * periods);
            let timing_penalty = 3.2 * ratio.log2().powi(2);
            let skip_penalty = 0.18 * (periods - 1.0);
            let candidate = scores[previous] + f64::from(strength) - timing_penalty - skip_penalty;
            if candidate > scores[index] {
                scores[index] = candidate;
                predecessors[index] = Some(previous);
                chain_lengths[index] = chain_lengths[previous] + 1;
            }
        }
    }

    let Some((terminal, score)) = scores
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.partial_cmp(right.1).unwrap_or(Ordering::Equal))
    else {
        return 0.0;
    };
    let mut chain = Vec::new();
    let mut cursor = Some(terminal);
    while let Some(index) = cursor {
        chain.push(peaks[index]);
        cursor = predecessors[index];
    }
    chain.reverse();
    if chain.len() < 4 {
        return 0.0;
    }
    let chain_strength = (score / chain.len() as f64).clamp(0.0, 1.0);
    let expected_beats = (envelope.len() as f64 / expected_period).max(1.0);
    let coverage = (chain_lengths[terminal] as f64 / expected_beats).clamp(0.0, 1.0);
    let timing_stability = beat_sequence_timing_stability(&chain, expected_period);
    (0.45 * chain_strength + 0.35 * coverage + 0.20 * timing_stability).clamp(0.0, 1.0)
}

fn beat_sequence_timing_stability(chain: &[(usize, f32)], expected_period: f64) -> f64 {
    if chain.len() < 3 {
        return 0.0;
    }
    let mut errors = Vec::with_capacity(chain.len() - 1);
    for pair in chain.windows(2) {
        let interval = (pair[1].0 - pair[0].0) as f64;
        let periods = (interval / expected_period).round().max(1.0);
        let expected = periods * expected_period;
        errors.push((interval - expected).abs() / expected.max(f64::EPSILON));
    }
    let mean_error = errors.iter().sum::<f64>() / errors.len() as f64;
    (1.0 - mean_error * 4.0).clamp(0.0, 1.0)
}

fn section_consistency(envelope_len: usize, beat_positions: &[f64], beat_strengths: &[f64]) -> f64 {
    if envelope_len == 0 || beat_positions.len() != beat_strengths.len() {
        return 0.0;
    }
    let section_count = 8_usize.min((beat_positions.len() / 4).max(1));
    let section_len = envelope_len as f64 / section_count as f64;
    let mut section_totals = vec![0.0_f64; section_count];
    let mut section_counts = vec![0_usize; section_count];
    for (&position, &strength) in beat_positions.iter().zip(beat_strengths) {
        let section = (position / section_len).floor() as usize;
        let section = section.min(section_count - 1);
        section_totals[section] += strength;
        section_counts[section] += 1;
    }
    let section_means: Vec<_> = section_totals
        .into_iter()
        .zip(section_counts)
        .filter_map(|(total, count)| (count >= 2).then_some(total / count as f64))
        .collect();
    if section_means.is_empty() {
        return 0.0;
    }
    let active_sections =
        section_means.iter().filter(|mean| **mean >= 0.08).count() as f64 / section_count as f64;
    let mean = section_means.iter().sum::<f64>() / section_means.len() as f64;
    let variance = section_means
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / section_means.len() as f64;
    let stability = 1.0 / (1.0 + variance.sqrt());
    (0.55 * active_sections + 0.45 * stability).clamp(0.0, 1.0)
}

fn relative_bpm_error(left: f64, right: f64) -> f64 {
    (left - right).abs() / left.max(right).max(f64::EPSILON)
}

fn neighborhood_max(values: &[f64], center: usize, radius: usize) -> f64 {
    let start = center.saturating_sub(radius);
    let end = center
        .saturating_add(radius)
        .saturating_add(1)
        .min(values.len());
    values
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .copied()
        .fold(0.0, f64::max)
}

fn refine_tempo_from_peaks(envelope: &[f32], expected_period: f64) -> Option<f64> {
    let minimum_distance = (expected_period * 0.55).floor().max(1.0) as usize;
    let mut peaks = Vec::new();
    for index in 1..envelope.len().saturating_sub(1) {
        if envelope[index] < 0.15
            || envelope[index] < envelope[index - 1]
            || envelope[index] < envelope[index + 1]
        {
            continue;
        }
        if let Some(previous) = peaks.last().copied() {
            if index - previous < minimum_distance {
                if envelope[index] > envelope[previous] {
                    *peaks.last_mut().expect("peak exists") = index;
                }
                continue;
            }
        }
        peaks.push(index);
    }
    if peaks.len() < 4 {
        return None;
    }

    let mut beat_numbers = Vec::with_capacity(peaks.len());
    beat_numbers.push(0.0_f64);
    for pair in peaks.windows(2) {
        let periods = ((pair[1] - pair[0]) as f64 / expected_period)
            .round()
            .max(1.0);
        beat_numbers.push(beat_numbers.last().copied().unwrap_or(0.0) + periods);
    }
    let mean_beat = beat_numbers.iter().sum::<f64>() / beat_numbers.len() as f64;
    let mean_frame = peaks.iter().map(|value| *value as f64).sum::<f64>() / peaks.len() as f64;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (&beat, &frame) in beat_numbers.iter().zip(&peaks) {
        let centered_beat = beat - mean_beat;
        covariance += centered_beat * (frame as f64 - mean_frame);
        variance += centered_beat * centered_beat;
    }
    let period = covariance / variance;
    (period.is_finite() && period >= expected_period * 0.85 && period <= expected_period * 1.15)
        .then_some(period)
}

fn refine_peak(correlations: &[f64], lag: usize) -> f64 {
    let Some((&left, rest)) = correlations.get(lag - 1).zip(correlations.get(lag..)) else {
        return lag as f64;
    };
    let Some((&center, &right)) = rest.first().zip(rest.get(1)) else {
        return lag as f64;
    };
    let denominator = left - 2.0 * center + right;
    if denominator.abs() <= f64::EPSILON {
        lag as f64
    } else {
        lag as f64 + (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
    }
}

fn normalized_correlation(envelope: &[f32], lag: usize) -> f64 {
    let mut dot = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (&left, &right) in envelope[lag..]
        .iter()
        .zip(&envelope[..envelope.len() - lag])
    {
        let left = f64::from(left);
        let right = f64::from(right);
        dot += left * right;
        left_energy += left * left;
        right_energy += right * right;
    }
    if left_energy <= f64::EPSILON || right_energy <= f64::EPSILON {
        0.0
    } else {
        dot / (left_energy * right_energy).sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::fixtures::{click_track, patterned_click_track, syncopated_click_track};

    #[test]
    fn detects_synthetic_click_tempos() {
        for expected in [60.0_f64, 90.0, 120.0, 128.0, 150.0, 180.0] {
            let mut analyzer = RhythmAnalyzer::new();
            let result = analyzer.analyze(&click_track(expected as f32, 24)).unwrap();
            let measured = result.bpm.unwrap().value;
            assert!(
                (measured - expected).abs() / expected <= 0.01,
                "expected {expected}, measured {measured}, candidates={:?}",
                result.candidates
            );
        }
    }

    #[test]
    fn silence_has_no_tempo() {
        let mut analyzer = RhythmAnalyzer::new();
        let result = analyzer
            .analyze(&vec![0.0; ANALYSIS_SAMPLE_RATE as usize * 8])
            .unwrap();
        assert!(result.bpm.is_none());
        assert!(result.candidates.is_empty());
    }

    #[test]
    fn resolves_accented_and_syncopated_half_double_ambiguity() {
        for signal in [
            patterned_click_track(120.0, 24, &[1.0, 0.28, 0.65, 0.28]),
            syncopated_click_track(120.0, 24),
        ] {
            let result = RhythmAnalyzer::new().analyze(&signal).unwrap();
            let measured = result.bpm.unwrap().value;
            assert!(
                (measured - 120.0).abs() / 120.0 <= 0.01,
                "measured {measured}, candidates={:?}",
                result.candidates
            );
        }
    }

    #[test]
    fn onset_envelope_is_finite_and_peaked() {
        let mut analyzer = RhythmAnalyzer::new();
        let result = analyzer.analyze(&click_track(120.0, 8)).unwrap();
        assert!(result.onset_envelope.iter().all(|value| value.is_finite()));
        assert!(result.onset_envelope.iter().any(|value| *value > 0.9));
    }

    #[test]
    fn sample_envelope_out_of_range_returns_silence() {
        let envelope = vec![0.2, 0.8, 0.4];
        assert_eq!(
            sample_envelope(&envelope, envelope.len() as f64 + 10.0),
            0.0
        );
        assert_eq!(sample_envelope(&[], 1.0), 0.0);
    }

    #[test]
    fn tempo_confidence_is_capped_for_ambiguous_candidates() {
        assert!(tempo_confidence(1.0, 0.8) < 0.65);
        assert!(tempo_confidence(1.0, 0.2) > 0.9);
    }

    #[test]
    fn tempo_state_support_prefers_consistent_tempo_over_wrong_tempo() {
        let result = RhythmAnalyzer::new()
            .analyze(&patterned_click_track(120.0, 32, &[1.0, 0.35, 0.70, 0.35]))
            .unwrap();
        let correct = tempo_state_support(&result.onset_envelope, 120.0);
        let wrong = tempo_state_support(&result.onset_envelope, 93.0);
        assert!(
            correct > wrong + 0.20,
            "correct={correct:.3}, wrong={wrong:.3}"
        );
        assert!(correct > 0.70, "correct={correct:.3}");
    }

    #[test]
    fn comb_filter_support_prefers_correct_tempo() {
        let result = RhythmAnalyzer::new()
            .analyze(&patterned_click_track(128.0, 32, &[1.0, 0.25, 0.65, 0.25]))
            .unwrap();
        let correct = comb_filter_support(&result.onset_envelope, 128.0);
        let wrong = comb_filter_support(&result.onset_envelope, 97.0);
        assert!(
            correct > wrong + 0.20,
            "correct={correct:.3}, wrong={wrong:.3}"
        );
        assert!(correct > 0.45, "correct={correct:.3}");
    }

    #[test]
    fn beat_sequence_support_prefers_correct_tempo() {
        let result = RhythmAnalyzer::new()
            .analyze(&patterned_click_track(150.0, 32, &[1.0, 0.30, 0.60, 0.30]))
            .unwrap();
        let correct = beat_sequence_support(&result.onset_envelope, 150.0);
        let wrong = beat_sequence_support(&result.onset_envelope, 113.0);
        assert!(
            correct > wrong + 0.20,
            "correct={correct:.3}, wrong={wrong:.3}"
        );
        assert!(correct > 0.60, "correct={correct:.3}");
    }

    #[test]
    fn octave_ambiguity_caps_confidence_without_forcing_range() {
        let mut candidates = vec![
            TempoCandidate::new(76.0, 0.60),
            TempoCandidate::new(152.0, 0.57),
        ];
        mark_octave_ambiguity(&mut candidates);
        assert_eq!(candidates[0].bpm, 76.0);
        assert!(candidates[0].grid.octave_ambiguous);
        assert!(tempo_confidence_for_candidates(&candidates) < 0.65);

        let mut clearly_supported_double = vec![
            TempoCandidate::new(152.0, 0.80),
            TempoCandidate::new(76.0, 0.60),
        ];
        mark_octave_ambiguity(&mut clearly_supported_double);
        assert_eq!(clearly_supported_double[0].bpm, 152.0);
        assert!(!clearly_supported_double[0].grid.octave_ambiguous);
    }

    #[test]
    fn tempo_octave_resolver_prefers_time_consistent_double_when_scores_are_close() {
        let mut half = TempoCandidate::new(76.0, 0.69);
        half.grid = CandidateGridSupport {
            tempo_state: 0.52,
            section_consistency: 0.99,
            band_consensus: 0.73,
            ..CandidateGridSupport::empty()
        };
        let mut double = TempoCandidate::new(152.0, 0.66);
        double.grid = CandidateGridSupport {
            tempo_state: 0.61,
            section_consistency: 0.99,
            band_consensus: 0.72,
            ..CandidateGridSupport::empty()
        };
        let mut candidates = vec![half, double];
        resolve_tempo_octaves(&mut candidates);
        assert_eq!(candidates[0].bpm, 152.0);
        assert!(candidates[0].grid.octave_preference > candidates[1].grid.octave_preference);
    }

    #[test]
    fn precision_tie_breaker_prefers_stronger_grid_when_scores_are_nearly_equal() {
        let mut weak_winner = TempoCandidate::new(122.0, 0.687);
        weak_winner.grid = CandidateGridSupport {
            score: 0.50,
            beat_strength: 0.11,
            tempo_state: 0.48,
            octave_preference: 0.687,
            ..CandidateGridSupport::empty()
        };
        let mut stronger_runner_up = TempoCandidate::new(115.0, 0.683);
        stronger_runner_up.grid = CandidateGridSupport {
            score: 0.545,
            beat_strength: 0.18,
            tempo_state: 0.525,
            octave_preference: 0.683,
            ..CandidateGridSupport::empty()
        };
        let mut candidates = vec![weak_winner, stronger_runner_up];
        apply_precision_tie_breakers(&mut candidates);
        candidates.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(Ordering::Equal)
        });
        assert_eq!(candidates[0].bpm, 115.0);
    }

    #[test]
    fn streamed_and_one_shot_analysis_match() {
        let signal = syncopated_click_track(128.0, 24);
        let direct = RhythmAnalyzer::new().analyze(&signal).unwrap();
        let mut streamed = RhythmAnalyzer::new();
        for chunk in signal.chunks(997) {
            streamed.push_signal(chunk).unwrap();
            assert!(streamed.pending.len() <= FFT_SIZE + 997);
        }
        let streamed = streamed.finish().unwrap();
        assert_eq!(streamed.bpm, direct.bpm);
        assert_eq!(streamed.candidates, direct.candidates);
        assert_eq!(streamed.onset_envelope, direct.onset_envelope);
        assert_eq!(streamed.beat_grid, direct.beat_grid);
    }

    #[test]
    fn beat_grid_tracks_clicks_within_twenty_milliseconds() {
        for expected in [60.0_f64, 90.0, 120.0, 128.0, 150.0, 180.0] {
            let result = RhythmAnalyzer::new()
                .analyze(&click_track(expected as f32, 24))
                .unwrap();
            let grid = result.beat_grid.unwrap();
            let period = f64::from(ANALYSIS_SAMPLE_RATE) * 60.0 / expected;
            let mut errors: Vec<f64> = grid
                .beats
                .iter()
                .map(|beat| {
                    let nearest = (beat.analysis_frame as f64 / period).round() * period;
                    (beat.analysis_frame as f64 - nearest).abs() * 1_000.0
                        / f64::from(ANALYSIS_SAMPLE_RATE)
                })
                .collect();
            errors.sort_by(|left, right| left.partial_cmp(right).unwrap());
            let median = errors[errors.len() / 2];
            assert!(
                median <= 20.0,
                "expected {expected} BPM, median timing error was {median:.3} ms"
            );
            assert!(grid.confidence >= 0.65);
        }
    }

    #[test]
    fn downbeats_require_clear_four_beat_accents() {
        let accented = RhythmAnalyzer::new()
            .analyze(&patterned_click_track(120.0, 24, &[1.0, 0.25, 0.55, 0.25]))
            .unwrap()
            .beat_grid
            .unwrap();
        assert!(accented.downbeat_confidence.is_some());
        assert!(accented.beats.iter().any(|beat| beat.downbeat));

        let unaccented = RhythmAnalyzer::new()
            .analyze(&click_track(120.0, 24))
            .unwrap()
            .beat_grid
            .unwrap();
        assert_eq!(unaccented.downbeat_confidence, None);
        assert!(unaccented.beats.iter().all(|beat| !beat.downbeat));
    }
}
