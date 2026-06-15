use signalsmith_stretch::Stretch;
use std::env;
use std::f32::consts::TAU;
use std::thread;
use std::time::{Duration, Instant};

const CHANNELS: usize = 2;
const SAMPLE_RATE: u32 = 48_000;
const BLOCK_FRAMES: usize = 512;
const TEMPO_RATIOS: [f64; 5] = [0.75, 0.8, 1.0, 1.25, 1.5];

#[derive(Clone, Copy, Debug)]
enum Preset {
    Default,
    Cheaper,
}

impl Preset {
    fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Cheaper => "cheaper",
        }
    }

    fn create(self) -> Stretch {
        match self {
            Self::Default => Stretch::preset_default(CHANNELS as u32, SAMPLE_RATE),
            Self::Cheaper => Stretch::preset_cheaper(CHANNELS as u32, SAMPLE_RATE),
        }
    }
}

#[derive(Debug)]
struct StreamResult {
    input_frames: u64,
    output_frames: u64,
    elapsed: Duration,
    deadline_misses: u64,
    simulated_underflows: u64,
    peak_block_time: Duration,
    finite: bool,
}

fn main() -> Result<(), String> {
    let soak_seconds = parse_soak_seconds()?;
    println!(
        "Signalsmith spike: arm={} sample_rate={} channels={} block_frames={} soak_seconds={}",
        env::consts::ARCH,
        SAMPLE_RATE,
        CHANNELS,
        BLOCK_FRAMES,
        soak_seconds
    );

    for preset in [Preset::Default, Preset::Cheaper] {
        report_latency(preset);
        for ratio in TEMPO_RATIOS {
            let result = run_stream(preset, ratio, 0.0, 12.0, 220.0)?;
            report_stream(preset, ratio, &result);
        }
        run_pitch_accuracy(preset)?;
        run_reset_flush_stress(preset)?;
    }

    run_two_deck_soak(Preset::Default, soak_seconds)?;
    Ok(())
}

fn parse_soak_seconds() -> Result<f64, String> {
    let mut args = env::args().skip(1);
    let mut soak_seconds = 60.0;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--soak-seconds" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--soak-seconds requires a value".to_owned())?;
                soak_seconds = value
                    .parse::<f64>()
                    .map_err(|_| format!("invalid soak duration: {value}"))?;
                if !soak_seconds.is_finite() || soak_seconds <= 0.0 {
                    return Err("soak duration must be a positive finite number".to_owned());
                }
            }
            "--help" | "-h" => {
                println!("Usage: djapp-signalsmith-spike [--soak-seconds NUMBER]");
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(soak_seconds)
}

fn report_latency(preset: Preset) {
    let stretch = preset.create();
    println!(
        "latency preset={} input_frames={} output_frames={} total_ms={:.3}",
        preset.name(),
        stretch.input_latency(),
        stretch.output_latency(),
        1_000.0 * (stretch.input_latency() + stretch.output_latency()) as f64 / SAMPLE_RATE as f64
    );
}

fn report_stream(preset: Preset, ratio: f64, result: &StreamResult) {
    let audio_seconds = result.output_frames as f64 / SAMPLE_RATE as f64;
    let realtime_factor = result.elapsed.as_secs_f64() / audio_seconds;
    let measured_ratio = result.input_frames as f64 / result.output_frames as f64;
    println!(
        "tempo preset={} ratio={ratio:.3} measured_ratio={measured_ratio:.6} audio_s={audio_seconds:.1} wall_s={:.3} realtime_factor={realtime_factor:.4} deadline_misses={} simulated_underflows={} peak_block_ms={:.3} finite={}",
        preset.name(),
        result.elapsed.as_secs_f64(),
        result.deadline_misses,
        result.simulated_underflows,
        result.peak_block_time.as_secs_f64() * 1_000.0,
        result.finite
    );
}

fn run_stream(
    preset: Preset,
    ratio: f64,
    semitones: f32,
    output_seconds: f64,
    frequency: f32,
) -> Result<StreamResult, String> {
    let mut stretch = preset.create();
    stretch.set_transpose_factor_semitones(semitones, None);

    let output_frames_target = (output_seconds * SAMPLE_RATE as f64).round() as u64;
    let mut output_frames = 0_u64;
    let mut input_frames = 0_u64;
    let mut exact_input_position = 0.0_f64;
    let mut phase = 0.0_f32;
    let phase_step = TAU * frequency / SAMPLE_RATE as f32;
    let mut deadline_misses = 0_u64;
    let mut simulated_underflows = 0_u64;
    let mut peak_block_time = Duration::ZERO;
    let mut finite = true;
    let started = Instant::now();

    while output_frames < output_frames_target {
        let frames_out = BLOCK_FRAMES.min((output_frames_target - output_frames) as usize);
        exact_input_position += frames_out as f64 * ratio;
        let frames_in = exact_input_position.floor() as u64 - input_frames;
        let mut input = vec![0.0_f32; frames_in as usize * CHANNELS];
        fill_stereo_tone(&mut input, &mut phase, phase_step);
        let mut output = vec![0.0_f32; frames_out * CHANNELS];

        let block_started = Instant::now();
        stretch.process(&input, &mut output);
        let block_time = block_started.elapsed();
        peak_block_time = peak_block_time.max(block_time);
        if block_time.as_secs_f64() > frames_out as f64 / SAMPLE_RATE as f64 {
            deadline_misses += 1;
        }
        finite &= output.iter().all(|sample| sample.is_finite());
        input_frames += frames_in;
        output_frames += frames_out as u64;
        let buffered_audio_seconds = (stretch.input_latency() + stretch.output_latency()) as f64
            / SAMPLE_RATE as f64
            + output_frames as f64 / SAMPLE_RATE as f64;
        if started.elapsed().as_secs_f64() > buffered_audio_seconds {
            simulated_underflows += 1;
        }
    }

    if !finite {
        return Err(format!(
            "{} preset generated non-finite output",
            preset.name()
        ));
    }
    Ok(StreamResult {
        input_frames,
        output_frames,
        elapsed: started.elapsed(),
        deadline_misses,
        simulated_underflows,
        peak_block_time,
        finite,
    })
}

fn fill_stereo_tone(buffer: &mut [f32], phase: &mut f32, phase_step: f32) {
    for frame in buffer.chunks_exact_mut(CHANNELS) {
        let sample = phase.sin() * 0.25;
        frame[0] = sample;
        frame[1] = sample;
        *phase = (*phase + phase_step) % TAU;
    }
}

fn run_pitch_accuracy(preset: Preset) -> Result<(), String> {
    for semitones in [-12.0_f32, -7.0, 0.0, 7.0, 12.0] {
        let input_frames = SAMPLE_RATE as usize * 4;
        let mut input = vec![0.0_f32; input_frames * CHANNELS];
        let mut phase = 0.0;
        fill_stereo_tone(&mut input, &mut phase, TAU * 440.0 / SAMPLE_RATE as f32);
        let mut output = vec![0.0_f32; input.len()];
        let mut stretch = preset.create();
        stretch.set_transpose_factor_semitones(semitones, None);
        if !stretch.exact(&input, &mut output) {
            return Err(format!("{} exact pitch processing failed", preset.name()));
        }

        let trim_frames = SAMPLE_RATE as usize;
        let measured = estimate_frequency(&output[trim_frames * CHANNELS..], SAMPLE_RATE as f64)?;
        let expected = 440.0 * 2.0_f64.powf(semitones as f64 / 12.0);
        let cents = 1_200.0 * (measured / expected).log2();
        println!(
            "pitch preset={} semitones={semitones:+.1} expected_hz={expected:.3} measured_hz={measured:.3} error_cents={cents:+.2}",
            preset.name()
        );
        if cents.abs() > 20.0 {
            return Err(format!(
                "{} pitch error exceeded 20 cents at {semitones} semitones",
                preset.name()
            ));
        }
    }
    Ok(())
}

fn estimate_frequency(interleaved: &[f32], sample_rate: f64) -> Result<f64, String> {
    let mono: Vec<f32> = interleaved
        .chunks_exact(CHANNELS)
        .map(|frame| frame[0])
        .collect();
    let crossings: Vec<usize> = mono
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair[0] <= 0.0 && pair[1] > 0.0).then_some(index + 1))
        .collect();
    if crossings.len() < 3 {
        return Err("not enough zero crossings to estimate pitch".to_owned());
    }
    let periods = crossings.last().unwrap() - crossings.first().unwrap();
    Ok(sample_rate * (crossings.len() - 1) as f64 / periods as f64)
}

fn run_reset_flush_stress(preset: Preset) -> Result<(), String> {
    let mut stretch = preset.create();
    let mut phase = 0.0;
    let mut finite = true;
    for index in 0..400 {
        let ratio = TEMPO_RATIOS[index % TEMPO_RATIOS.len()];
        let semitones = ((index % 25) as f32 - 12.0).clamp(-12.0, 12.0);
        stretch.set_transpose_factor_semitones(semitones, None);
        let input_frames = (BLOCK_FRAMES as f64 * ratio).round() as usize;
        let mut input = vec![0.0_f32; input_frames * CHANNELS];
        fill_stereo_tone(&mut input, &mut phase, TAU * 330.0 / SAMPLE_RATE as f32);
        let mut output = vec![0.0_f32; BLOCK_FRAMES * CHANNELS];
        stretch.process(&input, &mut output);
        finite &= output.iter().all(|sample| sample.is_finite());
        if index % 40 == 39 {
            stretch.reset();
            phase = 0.0;
        }
    }
    let mut tail = vec![0.0_f32; stretch.output_latency() * CHANNELS];
    stretch.flush(&mut tail);
    finite &= tail.iter().all(|sample| sample.is_finite());
    println!(
        "stress preset={} automation_blocks=400 resets=10 flush_frames={} finite={finite}",
        preset.name(),
        tail.len() / CHANNELS
    );
    finite.then_some(()).ok_or_else(|| {
        format!(
            "{} reset/flush stress generated invalid output",
            preset.name()
        )
    })
}

fn run_two_deck_soak(preset: Preset, seconds: f64) -> Result<(), String> {
    let started = Instant::now();
    let deck_a = thread::spawn(move || run_stream(preset, 0.8, 0.0, seconds, 220.0));
    let deck_b = thread::spawn(move || run_stream(preset, 1.25, 7.0, seconds, 330.0));
    let a = deck_a
        .join()
        .map_err(|_| "deck A worker panicked".to_owned())??;
    let b = deck_b
        .join()
        .map_err(|_| "deck B worker panicked".to_owned())??;
    println!(
        "two_deck_soak preset={} audio_s_per_deck={seconds:.1} wall_s={:.3} deadline_misses={} simulated_underflows={} peak_block_ms={:.3} finite={}",
        preset.name(),
        started.elapsed().as_secs_f64(),
        a.deadline_misses + b.deadline_misses,
        a.simulated_underflows + b.simulated_underflows,
        a.peak_block_time.max(b.peak_block_time).as_secs_f64() * 1_000.0,
        a.finite && b.finite
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_estimator_tracks_a_sine_wave() {
        let mut samples = vec![0.0; SAMPLE_RATE as usize * CHANNELS];
        let mut phase = 0.0;
        fill_stereo_tone(&mut samples, &mut phase, TAU * 440.0 / SAMPLE_RATE as f32);
        let measured = estimate_frequency(&samples, SAMPLE_RATE as f64).unwrap();
        assert!((measured - 440.0).abs() < 0.2, "measured {measured}");
    }

    #[test]
    fn streaming_ratio_tracks_fractional_input_frames() {
        let result = run_stream(Preset::Cheaper, 0.8, 0.0, 0.25, 220.0).unwrap();
        let measured = result.input_frames as f64 / result.output_frames as f64;
        assert!((measured - 0.8).abs() < 0.0001, "measured {measured}");
    }
}
