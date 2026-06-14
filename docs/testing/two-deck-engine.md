# Two-Deck Engine

The two-deck engine combines two independent decoder pipelines in one CoreAudio callback and master clock.

## Architecture

- Each deck owns its own decoder worker, generation, transport controls, PCM queue, recycle queue, and health metrics.
- One CPAL callback renders both decks for every output frame.
- Channel gain is applied in each deck render state.
- An equal-power crossfader mixes decks A and B.
- Master gain is applied after crossfading.
- Crossfader coefficients are recalculated only when the control changes, not per sample.
- Output is clamped to `[-1.0, 1.0]`, and samples that would clip are counted.

## Commands

Run a silent simultaneous playback test:

```sh
cargo run --release -- \
  --mix-media-a tests/fixtures/audio/tone.wav \
  --mix-media-b tests/fixtures/audio/tone.m4a \
  --crossfader 0.0 \
  --master-gain 1.0 \
  --gain 0.0
```

The crossfader range is `-1.0` for deck A through `1.0` for deck B. Channel gain is currently shared by the CLI test command, while the library API controls each deck independently.

## Automated Coverage

- Equal-power crossfader endpoints and center.
- Deterministic crossfader and master-gain output.
- Mixer control clamping.
- Existing deck tests cover play, pause, seek generations, EOF, gain, stale-block rejection, and worker track replacement.
- Existing decoder tests cover all supported media formats.

## Apple M3 Result

On macOS 26.4.1, deck A played the WAV fixture while deck B played the AAC-LC/M4A fixture through the MacBook Pro Speakers at stereo 44.1 kHz.

- Shared callbacks: 263.
- Deck A rendered 132,300 frames and reached EOF.
- Deck B rendered 134,144 frames and reached EOF.
- Underflow callbacks: 0 for both decks.
- Clipped samples: 0.
- Stream, recycling, and worker errors: 0.

The hardware test used channel gain `0.0` and was silent.

## Current Limits

- Both tracks must match the output-device sample rate.
- Tempo, sync, cue routing, EQ, filters, loops, and effects are not included yet.
- The current test is a short fixture smoke test, not a multi-hour performance benchmark.

