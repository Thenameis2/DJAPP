use std::{
    error::Error,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut manifest = None;
    let mut aubiotrack = PathBuf::from("aubiotrack");
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--manifest") => {
                let Some(path) = args.next() else {
                    return Err("usage: aubio_compare --manifest /path/to/private-bpm.tsv [--aubiotrack /path/to/aubiotrack]".into());
                };
                manifest = Some(PathBuf::from(path));
            }
            Some("--aubiotrack") => {
                let Some(path) = args.next() else {
                    return Err("--aubiotrack requires a command path".into());
                };
                aubiotrack = PathBuf::from(path);
            }
            _ => {
                return Err("usage: aubio_compare --manifest /path/to/private-bpm.tsv [--aubiotrack /path/to/aubiotrack]".into());
            }
        }
    }
    let Some(manifest) = manifest else {
        return Err("usage: aubio_compare --manifest /path/to/private-bpm.tsv [--aubiotrack /path/to/aubiotrack]".into());
    };
    run_manifest(&manifest, &aubiotrack)
}

fn run_manifest(path: &Path, aubiotrack: &Path) -> Result<ExitCode, Box<dyn Error>> {
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

    println!("label,expected_bpm,aubio_bpm,error_percent,accepted,beat_count,interval_stability,allowed,notes");
    let mut accepted = 0_usize;
    for row in &rows {
        let track_path = row.resolve_path(base);
        let analysis = run_aubiotrack(aubiotrack, &track_path);
        let (bpm, error_percent, passed, beat_count, stability, notes) = match analysis {
            Ok(analysis) => {
                let error_percent = row.error_percent(analysis.bpm);
                let passed = error_percent <= 1.0;
                if passed {
                    accepted += 1;
                }
                (
                    Some(analysis.bpm),
                    Some(error_percent),
                    passed,
                    analysis.beat_count,
                    Some(analysis.interval_stability),
                    String::new(),
                )
            }
            Err(error) => (None, None, false, 0, None, error.to_string()),
        };
        println!(
            "{},{:.6},{},{},{},{},{},{},{}",
            csv_cell(&row.label),
            row.expected_bpm,
            bpm.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.6}")),
            error_percent.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.6}")),
            passed,
            beat_count,
            stability.map_or_else(|| "unavailable".to_string(), |value| format!("{value:.6}")),
            csv_cell(row.allowed.as_str()),
            csv_cell(if notes.is_empty() { &row.notes } else { &notes })
        );
    }
    let pass_rate = accepted as f64 * 100.0 / rows.len() as f64;
    io::stdout().flush()?;
    eprintln!(
        "aubio accepted={accepted}/{} ({pass_rate:.2}%) with <=1% BPM error or allowed half/double equivalent",
        rows.len()
    );
    Ok(if pass_rate >= 90.0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AubioBeatAnalysis {
    bpm: f64,
    beat_count: usize,
    interval_stability: f64,
}

fn run_aubiotrack(command: &Path, track_path: &Path) -> Result<AubioBeatAnalysis, Box<dyn Error>> {
    let output = Command::new(command)
        .arg("--input")
        .arg(track_path)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "aubiotrack failed with status {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let stdout = String::from_utf8(output.stdout)?;
    let beats = parse_aubio_timestamps(&stdout);
    estimate_bpm_from_beats(&beats).ok_or_else(|| "aubiotrack returned too few usable beats".into())
}

fn parse_aubio_timestamps(output: &str) -> Vec<f64> {
    let mut values: Vec<_> = output
        .split(|character: char| {
            character.is_whitespace() || matches!(character, ',' | ';' | '[' | ']')
        })
        .filter_map(|token| token.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .collect();
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|left, right| (*left - *right).abs() < 0.001);
    values
}

fn estimate_bpm_from_beats(beats: &[f64]) -> Option<AubioBeatAnalysis> {
    if beats.len() < 4 {
        return None;
    }
    let mut intervals: Vec<_> = beats
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|interval| interval.is_finite() && (0.05..=2.0).contains(interval))
        .collect();
    if intervals.len() < 3 {
        return None;
    }
    intervals.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let median = percentile(&intervals, 0.5);
    if !median.is_finite() || median <= 0.0 {
        return None;
    }
    let mut bpm = 60.0 / median;
    while bpm < MIN_BPM {
        bpm *= 2.0;
    }
    while bpm > MAX_BPM {
        bpm /= 2.0;
    }
    let deviations: Vec<_> = intervals
        .iter()
        .map(|interval| (interval - median).abs())
        .collect();
    let mad = percentile(&deviations, 0.5);
    let stability = (1.0 - (mad / median.max(f64::EPSILON))).clamp(0.0, 1.0);
    Some(AubioBeatAnalysis {
        bpm,
        beat_count: beats.len(),
        interval_stability: stability,
    })
}

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    let index = ((sorted.len() - 1) as f64 * fraction.clamp(0.0, 1.0)).round() as usize;
    sorted[index]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aubiotrack_timestamps_from_plain_output() {
        let beats = parse_aubio_timestamps("0.500\n1.000\n1.500\n2.000\n");
        assert_eq!(beats, vec![0.5, 1.0, 1.5, 2.0]);
    }

    #[test]
    fn estimates_bpm_from_regular_beats() {
        let analysis = estimate_bpm_from_beats(&[0.0, 0.5, 1.0, 1.5, 2.0]).unwrap();
        assert!((analysis.bpm - 120.0).abs() < 0.001);
        assert_eq!(analysis.beat_count, 5);
        assert!(analysis.interval_stability > 0.99);
    }

    #[test]
    fn folds_slow_intervals_into_dj_range() {
        let analysis = estimate_bpm_from_beats(&[0.0, 1.0, 2.0, 3.0, 4.0]).unwrap();
        assert!((analysis.bpm - 60.0).abs() < 0.001);
    }

    #[test]
    fn manifest_error_respects_allowed_octaves() {
        let row = ManifestRow::parse(1, "76.0\ttrack.wav\tany")
            .unwrap()
            .unwrap();
        assert!(row.error_percent(152.0) <= f64::EPSILON);
    }
}
