use std::{env, path::PathBuf, process::ExitCode, time::Duration};

use djapp_audio_spike::{
    audio::{AudioCommand, AudioProbe, ProbeOptions},
    deck::{wait_until_ended, DeckTransport},
    dual_output::run_dual_output_probe,
    media::decode::MediaDecoder,
    mixer::{wait_until_both_ended, DeckId, MixerEngine},
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("audio spike failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args(env::args().skip(1))?;

    if let (Some(master), Some(cue)) = (
        options.dual_output_master.as_deref(),
        options.dual_output_cue.as_deref(),
    ) {
        let duration = options.duration.unwrap_or_else(|| Duration::from_secs(30));
        let report = run_dual_output_probe(master, cue, duration)?;
        println!("elapsed_s,master_frames,cue_frames,relative_frames,drift_from_baseline_frames");
        for sample in report.samples.iter().filter(|sample| {
            report.duration_seconds <= 120
                || sample.elapsed_seconds == 1
                || sample.elapsed_seconds % 60 == 0
                || sample.elapsed_seconds == report.duration_seconds
        }) {
            println!(
                "{},{},{},{},{}",
                sample.elapsed_seconds,
                sample.master_frames,
                sample.cue_frames,
                sample.relative_frames,
                sample.drift_from_baseline_frames
            );
        }
        println!("{}", report.summary());
        return Ok(());
    }

    if let (Some(deck_a), Some(deck_b)) =
        (options.mix_media_a.as_ref(), options.mix_media_b.as_ref())
    {
        let mut mixer = MixerEngine::open_default(deck_a, deck_b)?;
        println!("Deck A media: {:#?}", mixer.media(DeckId::A));
        println!("Deck B media: {:#?}", mixer.media(DeckId::B));
        mixer.set_channel_gain(DeckId::A, options.gain)?;
        mixer.set_channel_gain(DeckId::B, options.gain)?;
        mixer.set_crossfader(options.crossfader)?;
        mixer.set_master_gain(options.master_gain)?;
        mixer.play(DeckId::A)?;
        mixer.play(DeckId::B)?;
        let timeout = options.duration.unwrap_or_else(|| Duration::from_secs(30));
        if !wait_until_both_ended(&mixer, timeout) {
            mixer.stop(DeckId::A)?;
            mixer.stop(DeckId::B)?;
        }
        let report = mixer.shutdown();
        println!("{}", report.summary());
        return Ok(());
    }

    if let Some(path) = options.play_media.as_ref() {
        let mut deck = DeckTransport::open_default(path, options.gain)?;
        println!("Deck media: {:#?}", deck.media());
        deck.set_gain(options.gain)?;
        if let Some(seconds) = options.seek_seconds {
            deck.seek(seconds, false)?;
        }
        deck.play()?;
        let timeout = options.duration.unwrap_or_else(|| Duration::from_secs(30));
        let ended = wait_until_ended(&deck, timeout);
        if !ended {
            deck.stop()?;
        }
        let report = deck.shutdown();
        println!("{}", report.summary());
        return Ok(());
    }

    if let Some(path) = options.inspect_media.as_ref() {
        let mut decoder = MediaDecoder::open(path)?;
        println!("Media: {:#?}", decoder.info());
        if let Some(seconds) = options.seek_seconds {
            let actual = decoder.seek(seconds)?;
            println!("Seek requested={seconds:.3}s actual={actual:.3}s");
        }
        let chunk = decoder
            .next_chunk()?
            .ok_or("media contains no decodable audio")?;
        println!(
            "First PCM chunk: frames={}, channels={}, sample_rate={}, format=f32 interleaved",
            chunk.frames(),
            chunk.channels,
            chunk.sample_rate
        );
        return Ok(());
    }

    let probe = AudioProbe::new()?;

    probe.print_devices()?;

    if let Some(duration) = options.duration {
        println!("\nStarting default output callback for {duration:?}...");
        let mut running = probe.start_default_output(ProbeOptions {
            tone_hz: options.tone_hz,
            initial_gain: options.gain,
        })?;

        if duration > Duration::from_secs(1) {
            std::thread::sleep(duration / 2);
            running.try_send(AudioCommand::SetGain(options.gain * 0.5))?;
            std::thread::sleep(duration - (duration / 2));
        } else {
            std::thread::sleep(duration);
        }

        running.try_send(AudioCommand::Stop)?;
        let report = running.finish();
        println!("{}", report.summary());
    }

    Ok(())
}

#[derive(Debug, PartialEq)]
struct CliOptions {
    duration: Option<Duration>,
    tone_hz: Option<f32>,
    gain: f32,
    inspect_media: Option<PathBuf>,
    seek_seconds: Option<f64>,
    play_media: Option<PathBuf>,
    mix_media_a: Option<PathBuf>,
    mix_media_b: Option<PathBuf>,
    crossfader: f32,
    master_gain: f32,
    dual_output_master: Option<String>,
    dual_output_cue: Option<String>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<CliOptions, String> {
    let mut duration = None;
    let mut tone_hz = None;
    let mut gain = 0.05;
    let mut inspect_media = None;
    let mut seek_seconds = None;
    let mut play_media = None;
    let mut mix_media_a = None;
    let mut mix_media_b = None;
    let mut crossfader = 0.0;
    let mut master_gain = 1.0;
    let mut dual_output_master = None;
    let mut dual_output_cue = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run-seconds" => {
                let value = next_value(&mut args, "--run-seconds")?;
                let seconds = value
                    .parse::<u64>()
                    .map_err(|_| "--run-seconds must be a positive integer".to_string())?;
                if seconds == 0 {
                    return Err("--run-seconds must be greater than zero".to_string());
                }
                duration = Some(Duration::from_secs(seconds));
            }
            "--tone-hz" => {
                let value = next_value(&mut args, "--tone-hz")?;
                let frequency = value
                    .parse::<f32>()
                    .map_err(|_| "--tone-hz must be a number".to_string())?;
                if !(20.0..=20_000.0).contains(&frequency) {
                    return Err("--tone-hz must be between 20 and 20000".to_string());
                }
                tone_hz = Some(frequency);
            }
            "--gain" => {
                let value = next_value(&mut args, "--gain")?;
                gain = value
                    .parse::<f32>()
                    .map_err(|_| "--gain must be a number".to_string())?;
                if !(0.0..=0.25).contains(&gain) {
                    return Err("--gain must be between 0.0 and 0.25".to_string());
                }
            }
            "--inspect-media" => {
                inspect_media = Some(PathBuf::from(next_value(&mut args, "--inspect-media")?));
            }
            "--play-media" => {
                play_media = Some(PathBuf::from(next_value(&mut args, "--play-media")?));
            }
            "--mix-media-a" => {
                mix_media_a = Some(PathBuf::from(next_value(&mut args, "--mix-media-a")?));
            }
            "--mix-media-b" => {
                mix_media_b = Some(PathBuf::from(next_value(&mut args, "--mix-media-b")?));
            }
            "--crossfader" => {
                let value = next_value(&mut args, "--crossfader")?;
                crossfader = value
                    .parse::<f32>()
                    .map_err(|_| "--crossfader must be a number".to_string())?;
                if !(-1.0..=1.0).contains(&crossfader) {
                    return Err("--crossfader must be between -1.0 and 1.0".to_string());
                }
            }
            "--master-gain" => {
                let value = next_value(&mut args, "--master-gain")?;
                master_gain = value
                    .parse::<f32>()
                    .map_err(|_| "--master-gain must be a number".to_string())?;
                if !(0.0..=1.0).contains(&master_gain) {
                    return Err("--master-gain must be between 0.0 and 1.0".to_string());
                }
            }
            "--dual-output-master" => {
                dual_output_master = Some(next_value(&mut args, "--dual-output-master")?);
            }
            "--dual-output-cue" => {
                dual_output_cue = Some(next_value(&mut args, "--dual-output-cue")?);
            }
            "--seek-seconds" => {
                let value = next_value(&mut args, "--seek-seconds")?;
                let seconds = value
                    .parse::<f64>()
                    .map_err(|_| "--seek-seconds must be a number".to_string())?;
                if !seconds.is_finite() || seconds < 0.0 {
                    return Err("--seek-seconds must be a finite non-negative number".to_string());
                }
                seek_seconds = Some(seconds);
            }
            "--help" | "-h" => {
                print_help();
                return Ok(CliOptions {
                    duration: None,
                    tone_hz: None,
                    gain,
                    inspect_media: None,
                    seek_seconds: None,
                    play_media: None,
                    mix_media_a: None,
                    mix_media_b: None,
                    crossfader,
                    master_gain,
                    dual_output_master: None,
                    dual_output_cue: None,
                });
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    if tone_hz.is_some() && duration.is_none() {
        return Err("--tone-hz requires --run-seconds".to_string());
    }
    if seek_seconds.is_some() && inspect_media.is_none() && play_media.is_none() {
        return Err("--seek-seconds requires --inspect-media or --play-media".to_string());
    }
    if inspect_media.is_some() && duration.is_some() {
        return Err("--inspect-media cannot be combined with --run-seconds".to_string());
    }
    if inspect_media.is_some() && play_media.is_some() {
        return Err("--inspect-media cannot be combined with --play-media".to_string());
    }
    if mix_media_a.is_some() != mix_media_b.is_some() {
        return Err("--mix-media-a and --mix-media-b must be supplied together".to_string());
    }
    if dual_output_master.is_some() != dual_output_cue.is_some() {
        return Err(
            "--dual-output-master and --dual-output-cue must be supplied together".to_string(),
        );
    }
    if dual_output_master.is_some()
        && (inspect_media.is_some()
            || play_media.is_some()
            || mix_media_a.is_some()
            || tone_hz.is_some())
    {
        return Err("dual-output measurement cannot be combined with other modes".to_string());
    }
    if mix_media_a.is_some() && (inspect_media.is_some() || play_media.is_some()) {
        return Err(
            "two-deck mixing cannot be combined with media inspection or one-deck playback"
                .to_string(),
        );
    }
    if play_media.is_none() && duration.is_some() && tone_hz.is_none() {
        // Preserve the original silent callback mode.
    }

    Ok(CliOptions {
        duration,
        tone_hz,
        gain,
        inspect_media,
        seek_seconds,
        play_media,
        mix_media_a,
        mix_media_b,
        crossfader,
        master_gain,
        dual_output_master,
        dual_output_cue,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn print_help() {
    println!(
        "djapp-audio-spike\n\n\
         Lists macOS audio devices and optionally exercises the default output callback.\n\n\
         Usage:\n  \
           cargo run --release -- [--run-seconds N] [--tone-hz HZ] [--gain 0.0..0.25]\n\n\
           cargo run --release -- --inspect-media PATH [--seek-seconds SECONDS]\n\n\
           cargo run --release -- --play-media PATH [--run-seconds N] [--gain 0.0..0.25]\n\n\
           cargo run --release -- --mix-media-a PATH --mix-media-b PATH [--crossfader -1.0..1.0] [--master-gain 0.0..1.0] [--gain 0.0..0.25]\n\n\
           cargo run --release -- --dual-output-master NAME_OR_UID --dual-output-cue NAME_OR_UID [--run-seconds N]\n\n\
         The callback outputs silence unless --tone-hz is supplied."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_options() {
        let options = parse_args([
            "--run-seconds".to_string(),
            "4".to_string(),
            "--tone-hz".to_string(),
            "440".to_string(),
            "--gain".to_string(),
            "0.1".to_string(),
        ])
        .unwrap();

        assert_eq!(options.duration, Some(Duration::from_secs(4)));
        assert_eq!(options.tone_hz, Some(440.0));
        assert_eq!(options.gain, 0.1);
        assert_eq!(options.inspect_media, None);
        assert_eq!(options.seek_seconds, None);
        assert_eq!(options.play_media, None);
        assert_eq!(options.mix_media_a, None);
        assert_eq!(options.mix_media_b, None);
        assert_eq!(options.dual_output_master, None);
    }

    #[test]
    fn rejects_tone_without_duration() {
        let error = parse_args(["--tone-hz".to_string(), "440".to_string()]).unwrap_err();
        assert_eq!(error, "--tone-hz requires --run-seconds");
    }

    #[test]
    fn parses_media_inspection_options() {
        let options = parse_args([
            "--inspect-media".to_string(),
            "track.m4a".to_string(),
            "--seek-seconds".to_string(),
            "12.5".to_string(),
        ])
        .unwrap();

        assert_eq!(options.inspect_media, Some(PathBuf::from("track.m4a")));
        assert_eq!(options.seek_seconds, Some(12.5));
        assert_eq!(options.play_media, None);
    }

    #[test]
    fn parses_deck_playback_options() {
        let options = parse_args([
            "--play-media".to_string(),
            "track.wav".to_string(),
            "--run-seconds".to_string(),
            "5".to_string(),
            "--gain".to_string(),
            "0.0".to_string(),
        ])
        .unwrap();

        assert_eq!(options.play_media, Some(PathBuf::from("track.wav")));
        assert_eq!(options.duration, Some(Duration::from_secs(5)));
        assert_eq!(options.gain, 0.0);
    }

    #[test]
    fn parses_two_deck_options() {
        let options = parse_args([
            "--mix-media-a".to_string(),
            "a.wav".to_string(),
            "--mix-media-b".to_string(),
            "b.flac".to_string(),
            "--crossfader".to_string(),
            "0.25".to_string(),
            "--master-gain".to_string(),
            "0.5".to_string(),
        ])
        .unwrap();

        assert_eq!(options.mix_media_a, Some(PathBuf::from("a.wav")));
        assert_eq!(options.mix_media_b, Some(PathBuf::from("b.flac")));
        assert_eq!(options.crossfader, 0.25);
        assert_eq!(options.master_gain, 0.5);
    }

    #[test]
    fn parses_dual_output_measurement_options() {
        let options = parse_args([
            "--dual-output-master".to_string(),
            "MacBook Pro Speakers".to_string(),
            "--dual-output-cue".to_string(),
            "External Headphones".to_string(),
            "--run-seconds".to_string(),
            "30".to_string(),
        ])
        .unwrap();
        assert_eq!(
            options.dual_output_master.as_deref(),
            Some("MacBook Pro Speakers")
        );
        assert_eq!(
            options.dual_output_cue.as_deref(),
            Some("External Headphones")
        );
        assert_eq!(options.duration, Some(Duration::from_secs(30)));
    }
}
