#[allow(dead_code)]
mod support;

use std::{error::Error, path::PathBuf};

use support::{synthesize_music_fixture, write_pcm16_wav, CHANNELS, SAMPLE_RATE};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures/audio/music-like-48k.wav"));
    let samples = synthesize_music_fixture();
    write_pcm16_wav(&path, &samples, SAMPLE_RATE, CHANNELS)?;
    println!(
        "generated={} sample_rate={} channels={} frames={}",
        path.display(),
        SAMPLE_RATE,
        CHANNELS,
        samples.len() / CHANNELS
    );
    Ok(())
}
