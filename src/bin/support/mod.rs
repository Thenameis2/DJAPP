use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};

pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const FIXTURE_SECONDS: usize = 20;

pub fn synthesize_music_fixture() -> Vec<f32> {
    let frames = SAMPLE_RATE as usize * FIXTURE_SECONDS;
    let mut samples = Vec::with_capacity(frames * CHANNELS);
    let mut noise_state = 0x6d2b_79f5_u32;
    let beat_frames = SAMPLE_RATE as usize / 2;
    let bass_notes = [
        55.0_f32, 65.406, 73.416, 82.407, 61.735, 69.296, 77.782, 92.499,
    ];
    let lead_notes = [
        220.0_f32, 246.942, 293.665, 329.628, 261.626, 349.228, 391.995, 466.164,
    ];

    for frame in 0..frames {
        let time = frame as f32 / SAMPLE_RATE as f32;
        let beat = frame / beat_frames;
        let beat_phase = (frame % beat_frames) as f32 / beat_frames as f32;
        let section = beat / 8;

        let kick_phase = beat_phase * 18.0;
        let kick = (std::f32::consts::TAU * (72.0 - 34.0 * beat_phase) * time).sin()
            * (-kick_phase).exp()
            * 0.8;

        noise_state = noise_state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let noise = ((noise_state >> 8) as f32 / 16_777_215.0) * 2.0 - 1.0;
        let snare_phase = ((beat_phase - 0.5).max(0.0) * 28.0).min(28.0);
        let snare = if beat_phase >= 0.5 {
            noise * (-snare_phase).exp() * 0.38
        } else {
            0.0
        };

        let hat_step = ((frame * 4 / beat_frames) % 4) as f32;
        let hat_phase = ((frame * 4 % beat_frames) as f32 / beat_frames as f32) * 45.0;
        let hat = noise * (-hat_phase).exp() * (0.09 + hat_step * 0.008);

        let bass_frequency = bass_notes[(beat + section) % bass_notes.len()];
        let bass_envelope = (1.0 - beat_phase).powf(1.4);
        let bass = (std::f32::consts::TAU * bass_frequency * time).sin() * bass_envelope * 0.32;

        let chord_root = bass_notes[(section * 3) % bass_notes.len()] * 2.0;
        let chord = [1.0_f32, 1.259_921, 1.498_307]
            .iter()
            .enumerate()
            .map(|(voice, ratio)| {
                (std::f32::consts::TAU * chord_root * ratio * time + voice as f32 * 0.3).sin()
            })
            .sum::<f32>()
            * 0.075;

        let lead_frequency = lead_notes[(beat * 3 + section) % lead_notes.len()];
        let lead_gate = if (beat + section).is_multiple_of(3) {
            1.0
        } else {
            0.35
        };
        let lead = (std::f32::consts::TAU * lead_frequency * time
            + 0.7 * (std::f32::consts::TAU * 0.17 * time).sin())
        .sin()
            * lead_gate
            * 0.13;

        let marker = if frame % (SAMPLE_RATE as usize * 3 + 137) < 48 {
            let marker_frequency = 900.0 + section as f32 * 113.0;
            (std::f32::consts::TAU * marker_frequency * time).sin() * 0.18
        } else {
            0.0
        };

        let mono = (kick + snare + hat + bass + chord + lead + marker).clamp(-0.95, 0.95);
        let pan = 0.18 * (std::f32::consts::TAU * 0.071 * time).sin();
        samples.push(mono * (1.0 - pan));
        samples.push(mono * (1.0 + pan));
    }

    samples
}

pub fn write_pcm16_wav(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
    channels: usize,
) -> io::Result<()> {
    let channels = u16::try_from(channels)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too many channels"))?;
    let data_bytes = u32::try_from(samples.len() * 2)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "WAV is too large"))?;
    let mut file = File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36_u32 + data_bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&channels.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * u32::from(channels) * 2).to_le_bytes())?;
    file.write_all(&(channels * 2).to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16;
        file.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

#[derive(Debug)]
pub struct RepeatMatch {
    pub first_frame: usize,
    pub repeated_frame: usize,
    pub correlation: f64,
    pub normalized_error: f64,
}

pub fn strongest_repeat(
    samples: &[f32],
    channels: usize,
    window_frames: usize,
    hop_frames: usize,
    minimum_separation_frames: usize,
) -> Option<RepeatMatch> {
    let frame_count = samples.len() / channels;
    let starts: Vec<usize> = (0..frame_count.saturating_sub(window_frames))
        .step_by(hop_frames)
        .collect();
    let mut strongest: Option<RepeatMatch> = None;

    for (right_index, &right_start) in starts.iter().enumerate() {
        for &left_start in &starts[..right_index] {
            if right_start - left_start < minimum_separation_frames {
                continue;
            }
            let left = mono_window(samples, channels, left_start, window_frames);
            let right = mono_window(samples, channels, right_start, window_frames);
            let (correlation, normalized_error) = similarity(&left, &right);
            if correlation < 0.995 || normalized_error > 0.08 {
                continue;
            }
            let candidate = RepeatMatch {
                first_frame: left_start,
                repeated_frame: right_start,
                correlation,
                normalized_error,
            };
            if strongest
                .as_ref()
                .map(|current| candidate.correlation > current.correlation)
                .unwrap_or(true)
            {
                strongest = Some(candidate);
            }
        }
    }
    strongest
}

fn mono_window(samples: &[f32], channels: usize, start_frame: usize, frames: usize) -> Vec<f32> {
    samples[start_frame * channels..(start_frame + frames) * channels]
        .chunks_exact(channels)
        .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
        .collect()
}

fn similarity(left: &[f32], right: &[f32]) -> (f64, f64) {
    let left_mean = left.iter().map(|value| f64::from(*value)).sum::<f64>() / left.len() as f64;
    let right_mean = right.iter().map(|value| f64::from(*value)).sum::<f64>() / right.len() as f64;
    let mut dot = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    let mut squared_error = 0.0;
    for (&left, &right) in left.iter().zip(right) {
        let left = f64::from(left) - left_mean;
        let right = f64::from(right) - right_mean;
        dot += left * right;
        left_energy += left * left;
        right_energy += right * right;
        let difference = left - right;
        squared_error += difference * difference;
    }
    let scale = (left_energy * right_energy).sqrt().max(1e-12);
    let correlation = dot / scale;
    let normalized_error = (squared_error / left.len() as f64).sqrt()
        / ((left_energy / left.len() as f64).sqrt().max(1e-12));
    (correlation, normalized_error)
}
