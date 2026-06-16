use std::cmp::Ordering;

use djapp_audio_spike::analysis::{
    fixtures::{click_track, patterned_click_track, syncopated_click_track},
    rhythm::RhythmAnalyzer,
    signal::ANALYSIS_SAMPLE_RATE,
};

fn main() {
    println!("fixture,bpm,beats,median_error_ms,p95_error_ms,grid_confidence,downbeats");
    let fixtures = [
        ("click-60", 60.0, click_track(60.0, 24)),
        ("click-90", 90.0, click_track(90.0, 24)),
        ("click-120", 120.0, click_track(120.0, 24)),
        ("click-128", 128.0, click_track(128.0, 24)),
        ("click-150", 150.0, click_track(150.0, 24)),
        ("click-180", 180.0, click_track(180.0, 24)),
        (
            "accented-120",
            120.0,
            patterned_click_track(120.0, 24, &[1.0, 0.25, 0.55, 0.25]),
        ),
        ("syncopated-120", 120.0, syncopated_click_track(120.0, 24)),
    ];
    let mut failed = false;
    for (name, bpm, signal) in fixtures {
        let result = RhythmAnalyzer::new()
            .analyze(&signal)
            .expect("benchmark analysis failed");
        let Some(grid) = result.beat_grid else {
            eprintln!("{name}: no beat grid");
            failed = true;
            continue;
        };
        let period = f64::from(ANALYSIS_SAMPLE_RATE) * 60.0 / bpm;
        let mut errors: Vec<f64> = grid
            .beats
            .iter()
            .map(|beat| {
                let nearest = (beat.analysis_frame as f64 / period).round() * period;
                (beat.analysis_frame as f64 - nearest).abs() * 1_000.0
                    / f64::from(ANALYSIS_SAMPLE_RATE)
            })
            .collect();
        errors.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
        let median = percentile(&errors, 0.5);
        let p95 = percentile(&errors, 0.95);
        let downbeats = grid.beats.iter().filter(|beat| beat.downbeat).count();
        println!(
            "{name},{bpm:.3},{},{median:.6},{p95:.6},{:.6},{downbeats}",
            grid.beats.len(),
            grid.confidence
        );
        failed |= median > 20.0;
    }
    if failed {
        std::process::exit(1);
    }
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    let index = ((values.len().saturating_sub(1)) as f64 * percentile).round() as usize;
    values[index]
}
