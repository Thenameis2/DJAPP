use std::f32::consts::TAU;

pub const FIXTURE_RATE: u32 = 22_050;

pub fn click_track(bpm: f32, seconds: usize) -> Vec<f32> {
    patterned_click_track(bpm, seconds, &[1.0])
}

pub fn patterned_click_track(bpm: f32, seconds: usize, pattern: &[f32]) -> Vec<f32> {
    assert!(!pattern.is_empty(), "click pattern must not be empty");
    let frames = FIXTURE_RATE as usize * seconds;
    let beat_frames = (FIXTURE_RATE as f32 * 60.0 / bpm).round() as usize;
    let mut output = vec![0.0; frames];
    for (beat_index, beat) in (0..frames).step_by(beat_frames.max(1)).enumerate() {
        let amplitude = pattern[beat_index % pattern.len()];
        for offset in 0..128.min(frames - beat) {
            let envelope = (-(offset as f32) / 24.0).exp();
            output[beat + offset] = (TAU * 1_200.0 * offset as f32 / FIXTURE_RATE as f32).sin()
                * envelope
                * 0.9
                * amplitude;
        }
    }
    output
}

pub fn syncopated_click_track(bpm: f32, seconds: usize) -> Vec<f32> {
    let mut output = click_track(bpm, seconds);
    let beat_frames = (FIXTURE_RATE as f32 * 60.0 / bpm).round() as usize;
    for offbeat in (beat_frames / 2..output.len()).step_by(beat_frames.max(1)) {
        for offset in 0..96.min(output.len() - offbeat) {
            let envelope = (-(offset as f32) / 18.0).exp();
            output[offbeat + offset] +=
                (TAU * 2_100.0 * offset as f32 / FIXTURE_RATE as f32).sin() * envelope * 0.24;
        }
    }
    output
}

pub fn major_triad(root_hz: f32, seconds: usize) -> Vec<f32> {
    let frames = FIXTURE_RATE as usize * seconds;
    let ratios = [1.0, 1.259_921, 1.498_307];
    (0..frames)
        .map(|frame| {
            let time = frame as f32 / FIXTURE_RATE as f32;
            ratios
                .iter()
                .map(|ratio| (TAU * root_hz * ratio * time).sin())
                .sum::<f32>()
                / ratios.len() as f32
        })
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub enum TriadMode {
    Major,
    Minor,
}

pub fn triad(pitch_class: u8, mode: TriadMode, tuning_cents: f32, seconds: usize) -> Vec<f32> {
    assert!(pitch_class < 12);
    let root_midi = 60.0 + f32::from(pitch_class);
    let intervals = match mode {
        TriadMode::Major => [0.0, 4.0, 7.0],
        TriadMode::Minor => [0.0, 3.0, 7.0],
    };
    let frequencies = intervals.map(|interval| {
        440.0 * 2.0_f32.powf((root_midi + interval - 69.0 + tuning_cents / 100.0) / 12.0)
    });
    let frames = FIXTURE_RATE as usize * seconds;
    (0..frames)
        .map(|frame| {
            let time = frame as f32 / FIXTURE_RATE as f32;
            frequencies
                .iter()
                .map(|frequency| (TAU * frequency * time).sin())
                .sum::<f32>()
                / frequencies.len() as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_fixture_has_expected_beats() {
        let fixture = click_track(120.0, 4);
        assert_eq!(fixture.len(), FIXTURE_RATE as usize * 4);
        assert!(fixture[1].abs() > 0.01);
        assert!(fixture[FIXTURE_RATE as usize / 2 + 1].abs() > 0.01);
    }

    #[test]
    fn key_fixture_is_finite_and_non_silent() {
        let fixture = major_triad(261.625_55, 2);
        assert!(fixture.iter().all(|sample| sample.is_finite()));
        assert!(fixture.iter().any(|sample| sample.abs() > 0.5));
    }
}
