use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use djapp_audio_spike::{
    analysis::{fixtures::click_track, rhythm::RhythmAnalyzer, signal::AnalysisSignalBuilder},
    media::decode::MediaDecoder,
};

const ANALYSIS_RATE: usize = 22_050;

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().and_then(|arg| arg.to_str()) == Some("--manifest") {
        let Some(path) = args.get(1).map(PathBuf::from) else {
            return Err("usage: bpm_benchmark --manifest /path/to/private-bpm.tsv".into());
        };
        return run_manifest(&path);
    }
    if args.first().and_then(|arg| arg.to_str()) == Some("--segments") {
        let Some(path) = args.get(1).map(PathBuf::from) else {
            return Err("usage: bpm_benchmark --segments /path/to/track [seconds]".into());
        };
        let seconds = args
            .get(2)
            .and_then(|value| value.to_str())
            .map(str::parse)
            .transpose()?
            .unwrap_or(30.0);
        run_segments(&path, seconds)?;
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(path) = args.first().map(PathBuf::from) {
        let result = analyze_file(&path)?;
        println!("input={}", path.display());
        println!("estimate={:?}", result.bpm);
        for candidate in result.candidates {
            println!(
                "candidate_bpm={:.6},score={:.6}",
                candidate.bpm, candidate.score
            );
        }
        return Ok(ExitCode::SUCCESS);
    }
    let mut failed = false;
    println!("expected_bpm,measured_bpm,error_percent,confidence");
    for expected in [60.0_f64, 90.0, 120.0, 128.0, 150.0, 180.0] {
        let mut analyzer = RhythmAnalyzer::new();
        let result = analyzer.analyze(&click_track(expected as f32, 24))?;
        let Some(estimate) = result.bpm else {
            println!("{expected},unavailable,unavailable,0.000000");
            failed = true;
            continue;
        };
        let error_percent = (estimate.value - expected).abs() * 100.0 / expected;
        println!(
            "{expected:.3},{:.6},{error_percent:.6},{:.6}",
            estimate.value, estimate.confidence
        );
        failed |= error_percent > 0.1;
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn load_analysis_signal(path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut decoder = MediaDecoder::open(path)?;
    let info = decoder.info().clone();
    let mut signal = AnalysisSignalBuilder::new(info.sample_rate, info.channels)?;
    while let Some(chunk) = decoder.next_chunk()? {
        signal.push_interleaved(&chunk.samples)?;
    }
    Ok(signal.finish()?)
}

fn analyze_file(
    path: &Path,
) -> Result<djapp_audio_spike::analysis::rhythm::RhythmAnalysis, Box<dyn Error>> {
    Ok(RhythmAnalyzer::new().analyze(&load_analysis_signal(path)?)?)
}

fn run_segments(path: &Path, seconds: f64) -> Result<(), Box<dyn Error>> {
    if !seconds.is_finite() || seconds <= 0.0 {
        return Err("segment seconds must be positive".into());
    }
    let signal = load_analysis_signal(path)?;
    let segment_frames = (seconds * ANALYSIS_RATE as f64).round().max(1.0) as usize;
    let hop_frames = (segment_frames / 2).max(1);
    println!("start_seconds,end_seconds,measured_bpm,confidence,candidates");
    let mut start = 0;
    while start < signal.len() {
        let end = (start + segment_frames).min(signal.len());
        if end - start < segment_frames / 2 {
            break;
        }
        let result = RhythmAnalyzer::new().analyze(&signal[start..end])?;
        let (bpm, confidence) = result
            .bpm
            .map(|estimate| (format!("{:.6}", estimate.value), estimate.confidence))
            .unwrap_or_else(|| ("unavailable".to_string(), 0.0));
        println!(
            "{:.3},{:.3},{},{:.6},{}",
            start as f64 / ANALYSIS_RATE as f64,
            end as f64 / ANALYSIS_RATE as f64,
            bpm,
            confidence,
            csv_cell(&format_candidates(&result.candidates))
        );
        start += hop_frames;
    }
    Ok(())
}

fn run_manifest(path: &Path) -> Result<ExitCode, Box<dyn Error>> {
    let manifest = fs::read_to_string(path)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut rows = Vec::new();
    for (line_index, line) in manifest.lines().enumerate() {
        if let Some(row) = ManifestRow::parse(line_index + 1, line)? {
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return Err("manifest does not contain any benchmark rows".into());
    }

    println!("label,expected_bpm,measured_bpm,error_percent,confidence,accepted,nearest_candidate_bpm,nearest_candidate_error_percent,candidates,allowed,notes");
    let mut accepted = 0_usize;
    for row in &rows {
        let track_path = row.resolve_path(base);
        let result = analyze_file(&track_path);
        let (measured, confidence, error_percent, passed, candidates) = match result {
            Ok(result) => {
                let candidates = result.candidates;
                if let Some(estimate) = result.bpm {
                    let error_percent = row.error_percent(estimate.value);
                    let passed = error_percent <= 1.0;
                    (
                        Some(estimate.value),
                        estimate.confidence,
                        Some(error_percent),
                        passed,
                        candidates,
                    )
                } else {
                    (None, 0.0, None, false, candidates)
                }
            }
            Err(error) => {
                eprintln!("{}: {}", row.label, error);
                (None, 0.0, None, false, Vec::new())
            }
        };
        let nearest = nearest_candidate(row, &candidates);
        if passed {
            accepted += 1;
        }
        println!(
            "{},{:.6},{},{},{:.6},{},{},{},{},{},{}",
            csv_cell(&row.label),
            row.expected_bpm,
            measured.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.6}")),
            error_percent.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.6}")),
            confidence,
            passed,
            nearest
                .map(|candidate| format!("{:.6}", candidate.bpm))
                .unwrap_or_else(|| "unavailable".to_string()),
            nearest
                .map(|candidate| format!("{:.6}", row.error_percent(candidate.bpm)))
                .unwrap_or_else(|| "unavailable".to_string()),
            csv_cell(&format_candidates(&candidates)),
            csv_cell(row.allowed.as_str()),
            csv_cell(&row.notes)
        );
    }
    let pass_rate = accepted as f64 * 100.0 / rows.len() as f64;
    io::stdout().flush()?;
    eprintln!(
        "accepted={accepted}/{} ({pass_rate:.2}%) with <=1% BPM error or allowed half/double equivalent",
        rows.len()
    );
    Ok(if pass_rate >= 90.0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AllowedTempo {
    Exact,
    Half,
    Double,
    HalfOrDouble,
    Any,
}

impl AllowedTempo {
    fn parse(value: Option<&str>) -> Result<Self, Box<dyn Error>> {
        match value.map(str::trim).filter(|value| !value.is_empty()) {
            None | Some("exact") | Some("same") => Ok(Self::Exact),
            Some("half") => Ok(Self::Half),
            Some("double") => Ok(Self::Double),
            Some("half-or-double") | Some("half_double") => Ok(Self::HalfOrDouble),
            Some("any") => Ok(Self::Any),
            Some(value) => Err(format!("unknown allowed tempo value: {value}").into()),
        }
    }

    fn factors(self) -> &'static [f64] {
        match self {
            Self::Exact => &[1.0],
            Self::Half => &[0.5],
            Self::Double => &[2.0],
            Self::HalfOrDouble => &[0.5, 2.0],
            Self::Any => &[0.5, 1.0, 2.0],
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Half => "half",
            Self::Double => "double",
            Self::HalfOrDouble => "half-or-double",
            Self::Any => "any",
        }
    }
}

#[derive(Debug, PartialEq)]
struct ManifestRow {
    line: usize,
    expected_bpm: f64,
    path: PathBuf,
    allowed: AllowedTempo,
    notes: String,
    label: String,
}

impl ManifestRow {
    fn parse(line: usize, value: &str) -> Result<Option<Self>, Box<dyn Error>> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }
        let fields: Vec<_> = value.split('\t').map(str::trim).collect();
        if fields.len() < 2 {
            return Err(format!("manifest line {line} must contain expected_bpm<TAB>path").into());
        }
        let expected_bpm: f64 = fields[0]
            .parse()
            .map_err(|_| format!("manifest line {line} has invalid expected BPM"))?;
        if !expected_bpm.is_finite() || expected_bpm <= 0.0 {
            return Err(format!("manifest line {line} expected BPM must be positive").into());
        }
        let path = PathBuf::from(fields[1]);
        if path.as_os_str().is_empty() {
            return Err(format!("manifest line {line} path is empty").into());
        }
        let allowed = AllowedTempo::parse(fields.get(2).copied())?;
        let notes = fields.get(3).copied().unwrap_or_default().to_string();
        let label = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(fields[1])
            .to_string();
        Ok(Some(Self {
            line,
            expected_bpm,
            path,
            allowed,
            notes,
            label,
        }))
    }

    fn resolve_path(&self, base: &Path) -> PathBuf {
        if self.path.is_absolute() {
            self.path.clone()
        } else {
            base.join(&self.path)
        }
    }

    fn error_percent(&self, measured_bpm: f64) -> f64 {
        self.allowed
            .factors()
            .iter()
            .map(|factor| self.expected_bpm * factor)
            .map(|expected| (measured_bpm - expected).abs() * 100.0 / expected)
            .fold(f64::INFINITY, f64::min)
    }
}

fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn format_candidates(candidates: &[djapp_audio_spike::analysis::rhythm::TempoCandidate]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "{:.3}@{:.3}[grid={:.3},beat={:.3},contrast={:.3},stable={:.3},sections={:.3},bands={:.3},state={:.3},comb={:.3},seq={:.3},oct={:.3},fit={:+.2},amb={}]",
                candidate.bpm,
                candidate.score,
                candidate.grid.score,
                candidate.grid.beat_strength,
                candidate.grid.offbeat_contrast,
                candidate.grid.stability,
                candidate.grid.section_consistency,
                candidate.grid.band_consensus,
                candidate.grid.tempo_state,
                candidate.grid.comb_filter,
                candidate.grid.beat_sequence,
                candidate.grid.octave_preference,
                candidate.grid.precision_adjustment_percent,
                if candidate.grid.octave_ambiguous { "y" } else { "n" }
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn nearest_candidate<'a>(
    row: &ManifestRow,
    candidates: &'a [djapp_audio_spike::analysis::rhythm::TempoCandidate],
) -> Option<&'a djapp_audio_spike::analysis::rhythm::TempoCandidate> {
    candidates.iter().min_by(|left, right| {
        row.error_percent(left.bpm)
            .partial_cmp(&row.error_percent(right.bpm))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_rows_and_relative_paths() {
        let row = ManifestRow::parse(3, "128.5\tMusic/song one.mp3\tany\tintro has drums")
            .unwrap()
            .unwrap();
        assert_eq!(row.line, 3);
        assert_eq!(row.expected_bpm, 128.5);
        assert_eq!(row.path, PathBuf::from("Music/song one.mp3"));
        assert_eq!(row.allowed, AllowedTempo::Any);
        assert_eq!(row.notes, "intro has drums");
        assert_eq!(
            row.resolve_path(Path::new("/tmp/manifest")),
            PathBuf::from("/tmp/manifest/Music/song one.mp3")
        );
    }

    #[test]
    fn scores_allowed_half_and_double_tempos() {
        let row = ManifestRow::parse(1, "100.0\ttrack.wav\tany")
            .unwrap()
            .unwrap();
        assert!(row.error_percent(50.0) <= f64::EPSILON);
        assert!(row.error_percent(100.0) <= f64::EPSILON);
        assert!(row.error_percent(200.0) <= f64::EPSILON);
        assert!(row.error_percent(125.0) > 1.0);
    }

    #[test]
    fn formats_candidates_for_manifest_diagnostics() {
        let candidates = vec![
            djapp_audio_spike::analysis::rhythm::TempoCandidate::new(120.1234, 0.9876),
            djapp_audio_spike::analysis::rhythm::TempoCandidate::new(90.0, 0.5),
        ];
        assert_eq!(
            format_candidates(&candidates),
            "120.123@0.988[grid=0.000,beat=0.000,contrast=0.000,stable=0.000,sections=0.000,bands=0.000,state=0.000,comb=0.000,seq=0.000,oct=0.000,fit=+0.00,amb=n] 90.000@0.500[grid=0.000,beat=0.000,contrast=0.000,stable=0.000,sections=0.000,bands=0.000,state=0.000,comb=0.000,seq=0.000,oct=0.000,fit=+0.00,amb=n]"
        );
    }
}
