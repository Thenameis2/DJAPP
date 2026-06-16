use djapp_audio_spike::analysis::{
    fixtures::{triad, TriadMode},
    key::KeyAnalyzer,
    types::{MusicalKey, MusicalMode},
};

fn main() {
    println!("expected,measured,classification,confidence,tuning_cents");
    let mut exact = 0;
    let mut total = 0;
    for pitch_class in 0..12 {
        for (fixture_mode, mode) in [
            (TriadMode::Major, MusicalMode::Major),
            (TriadMode::Minor, MusicalMode::Minor),
        ] {
            let expected = MusicalKey::new(pitch_class, mode).expect("valid key");
            let result = KeyAnalyzer::new()
                .analyze(&triad(pitch_class, fixture_mode, 0.0, 5))
                .expect("key benchmark failed");
            let measured = result.key.map(|estimate| estimate.value);
            let classification = classify(expected, measured);
            exact += usize::from(classification == "exact");
            total += 1;
            println!(
                "{},{},{},{:.6},{:.3}",
                label(expected),
                measured.map(label).unwrap_or_else(|| "none".to_string()),
                classification,
                result.key.map_or(0.0, |estimate| estimate.confidence),
                result.tuning_cents.unwrap_or(0.0)
            );
        }
    }
    eprintln!("exact={exact}/{total}");
    if exact != total {
        std::process::exit(1);
    }
}

fn classify(expected: MusicalKey, measured: Option<MusicalKey>) -> &'static str {
    let Some(measured) = measured else {
        return "incorrect";
    };
    if measured == expected {
        "exact"
    } else if measured.pitch_class == expected.pitch_class || is_relative(expected, measured) {
        "relative-or-parallel"
    } else if camelot_distance(expected, measured) == 1 {
        "neighboring-camelot"
    } else {
        "incorrect"
    }
}

fn is_relative(left: MusicalKey, right: MusicalKey) -> bool {
    match (left.mode, right.mode) {
        (MusicalMode::Major, MusicalMode::Minor) => {
            right.pitch_class == (left.pitch_class + 9) % 12
        }
        (MusicalMode::Minor, MusicalMode::Major) => {
            right.pitch_class == (left.pitch_class + 3) % 12
        }
        _ => false,
    }
}

fn camelot_distance(left: MusicalKey, right: MusicalKey) -> u8 {
    let left = camelot_number(left);
    let right = camelot_number(right);
    left.abs_diff(right).min(12 - left.abs_diff(right))
}

fn camelot_number(key: MusicalKey) -> u8 {
    const MAJOR: [u8; 12] = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];
    const MINOR: [u8; 12] = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];
    match key.mode {
        MusicalMode::Major => MAJOR[usize::from(key.pitch_class)],
        MusicalMode::Minor => MINOR[usize::from(key.pitch_class)],
    }
}

fn label(key: MusicalKey) -> String {
    let mode = match key.mode {
        MusicalMode::Major => "major",
        MusicalMode::Minor => "minor",
    };
    format!("{}:{mode}", key.pitch_class)
}
