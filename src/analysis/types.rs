use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackIdentity {
    pub track_id: i64,
    pub path: PathBuf,
    pub file_size: u64,
    pub modified_at_ms: i64,
    pub content_fingerprint: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisStage {
    Queued,
    Decoding,
    Waveform,
    Rhythm,
    Key,
    Loudness,
    Writing,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate<T> {
    pub value: T,
    pub confidence: f32,
}

impl<T> Estimate<T> {
    pub fn new(value: T, confidence: f32) -> Option<Self> {
        (confidence.is_finite() && (0.0..=1.0).contains(&confidence))
            .then_some(Self { value, confidence })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicalMode {
    Major,
    Minor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicalKey {
    pub pitch_class: u8,
    pub mode: MusicalMode,
}

impl MusicalKey {
    pub fn new(pitch_class: u8, mode: MusicalMode) -> Option<Self> {
        (pitch_class < 12).then_some(Self { pitch_class, mode })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackAnalysisResult {
    pub analysis_version: u32,
    pub bpm: Option<Estimate<f64>>,
    pub musical_key: Option<Estimate<MusicalKey>>,
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: Option<f64>,
    pub waveform_path: Option<PathBuf>,
    pub beat_grid_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_reject_invalid_confidence() {
        assert!(Estimate::new(120.0, 0.75).is_some());
        assert!(Estimate::new(120.0, -0.1).is_none());
        assert!(Estimate::new(120.0, 1.1).is_none());
        assert!(Estimate::new(120.0, f32::NAN).is_none());
    }

    #[test]
    fn musical_keys_reject_invalid_pitch_classes() {
        assert!(MusicalKey::new(11, MusicalMode::Minor).is_some());
        assert!(MusicalKey::new(12, MusicalMode::Major).is_none());
    }
}
