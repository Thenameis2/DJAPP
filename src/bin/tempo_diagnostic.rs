#[allow(dead_code)]
mod support;

use std::{collections::VecDeque, error::Error, path::PathBuf};

use djapp_audio_spike::{
    media::{decode::MediaDecoder, resample::EngineRateDecoder},
    tempo::{TempoProcessor, TempoSettings},
};
use support::{strongest_repeat, write_pcm16_wav};

const ENGINE_RATE: u32 = 48_000;
const CHANGE_ONE_FRAME: u64 = ENGINE_RATE as u64 * 5;
const CHANGE_TWO_FRAME: u64 = ENGINE_RATE as u64 * 12;
const DIAGNOSTIC_FRAMES: u64 = ENGINE_RATE as u64 * 20;
const QUEUE_CAPACITY: usize = 16;
const CALLBACK_FRAMES: usize = 512;

fn main() -> Result<(), Box<dyn Error>> {
    let input = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tests/fixtures/audio/music-like-48k.wav"));
    let output = std::env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/tempo-diagnostic.wav"));

    let (rendered, channels, decoded_frames) = render_queued(&input)?;

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_pcm16_wav(&output, &rendered, ENGINE_RATE, channels)?;
    let repeat = strongest_repeat(
        &rendered,
        channels,
        ENGINE_RATE as usize / 2,
        ENGINE_RATE as usize / 8,
        ENGINE_RATE as usize,
    );
    println!(
        "mode=queued input={} output={} decoded_frames={} rendered_frames={} queue_blocks={} callback_frames={} changes=+8%@5s,-8%@12s",
        input.display(),
        output.display(),
        decoded_frames,
        rendered.len() / channels,
        QUEUE_CAPACITY,
        CALLBACK_FRAMES,
    );
    match repeat {
        Some(repeat) => {
            println!(
                "repeat_detected=true first_seconds={:.3} repeated_seconds={:.3} correlation={:.6} normalized_error={:.6}",
                repeat.first_frame as f64 / ENGINE_RATE as f64,
                repeat.repeated_frame as f64 / ENGINE_RATE as f64,
                repeat.correlation,
                repeat.normalized_error,
            );
            std::process::exit(2);
        }
        None => println!("repeat_detected=false"),
    }
    Ok(())
}

fn render_queued(input: &PathBuf) -> Result<(Vec<f32>, usize, u64), Box<dyn Error>> {
    let source = MediaDecoder::open(input)?;
    let mut decoder = EngineRateDecoder::new(source, ENGINE_RATE)?;
    let channels = decoder.info().channels;
    let mut processor = TempoProcessor::new(ENGINE_RATE, channels, TempoSettings::default())?;
    let mut queue = VecDeque::with_capacity(QUEUE_CAPACITY);
    let mut current = None;
    let mut current_offset = 0;
    let mut decoded_frames = 0_u64;
    let mut rendered_frames = 0_u64;
    let mut changed_once = false;
    let mut changed_twice = false;
    let mut decode_buffer = Vec::new();
    let mut decoder_eof = false;
    let mut processor_flushed = false;
    let mut rendered = Vec::new();

    loop {
        if !changed_once && rendered_frames >= CHANGE_ONE_FRAME {
            processor.set_settings(TempoSettings {
                tempo_percent: 8.0,
                ..TempoSettings::default()
            })?;
            changed_once = true;
        }
        if !changed_twice && rendered_frames >= CHANGE_TWO_FRAME {
            processor.set_settings(TempoSettings {
                tempo_percent: -8.0,
                ..TempoSettings::default()
            })?;
            changed_twice = true;
        }

        while queue.len() < QUEUE_CAPACITY && !processor_flushed {
            if decoder_eof {
                if let Some(chunk) = processor.flush(Vec::new()) {
                    queue.push_back(chunk);
                }
                processor_flushed = true;
                break;
            }
            match decoder.next_chunk_into(std::mem::take(&mut decode_buffer))? {
                Some(chunk) => {
                    decoded_frames += chunk.frames() as u64;
                    let (processed, input_buffer) = processor.process(chunk, Vec::new())?;
                    decode_buffer = input_buffer;
                    if let Some(chunk) = processed {
                        queue.push_back(chunk);
                    }
                }
                None => decoder_eof = true,
            }
        }

        let mut callback_remaining = CALLBACK_FRAMES;
        while callback_remaining > 0 {
            if current.is_none() {
                current = queue.pop_front();
                current_offset = 0;
            }
            let Some(chunk) = current.as_ref() else {
                break;
            };
            let available = chunk.frames() - current_offset;
            let remaining_diagnostic = DIAGNOSTIC_FRAMES.saturating_sub(rendered_frames) as usize;
            let frames = available.min(callback_remaining).min(remaining_diagnostic);
            if frames == 0 {
                break;
            }
            let start = current_offset * channels;
            let end = (current_offset + frames) * channels;
            rendered.extend_from_slice(&chunk.samples[start..end]);
            current_offset += frames;
            callback_remaining -= frames;
            rendered_frames += frames as u64;
            if current_offset == chunk.frames() {
                current = None;
            }
        }

        if rendered_frames >= DIAGNOSTIC_FRAMES {
            break;
        }

        if processor_flushed && queue.is_empty() && current.is_none() {
            break;
        }
        if callback_remaining == CALLBACK_FRAMES && queue.is_empty() && current.is_none() {
            return Err("queued tempo diagnostic stalled before EOF".into());
        }
    }

    Ok((rendered, channels, decoded_frames))
}
