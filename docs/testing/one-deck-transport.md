# One-Deck Transport

The one-deck transport connects Symphonia decoding to the CPAL output callback without performing file I/O, decoding, allocation, locking, or buffer destruction on the real-time thread.

## Architecture

- A decoder worker reads and converts local media into bounded interleaved `f32` PCM blocks.
- A fixed-capacity `rtrb` queue carries PCM blocks to the CoreAudio callback.
- A second fixed-capacity queue returns consumed `Vec<f32>` allocations to the worker for reuse.
- A bounded control queue carries play, pause, gain, and generation-reset commands.
- Seek, stop, and track replacement increment a generation. The callback discards blocks from older generations.
- EOF, readiness, position, callback, underflow, stale-block, recycling, stream-error, and worker-error state is reported through atomics or non-real-time state.

## Commands

Play a local file at a quiet gain:

```sh
cargo run --release -- --play-media tests/fixtures/audio/tone.wav --gain 0.05
```

Run a silent hardware smoke test:

```sh
cargo run --release -- --play-media tests/fixtures/audio/tone.m4a --gain 0.0
```

Seek before playback:

```sh
cargo run --release -- --play-media tests/fixtures/audio/tone.m4a --seek-seconds 1.5 --gain 0.0
```

`--run-seconds` provides a timeout. When omitted, the CLI waits up to 30 seconds for EOF.

## Automated Coverage

- Play and pause preserve the transport position correctly.
- Gain is applied in the callback.
- EOF changes the deck to `Ended`.
- Generation reset discards stale PCM.
- The decoder worker handles seek and track replacement.
- Invalid seek positions return errors.
- Existing decoder tests cover WAV, AIFF, FLAC, MP3, and AAC-LC/M4A.

## Apple M3 Results

On macOS 26.4.1 using the MacBook Pro Speakers at stereo 44.1 kHz:

- WAV, AIFF, FLAC, and MP3 each rendered 132,300 frames and reached EOF.
- AAC-LC/M4A rendered 134,144 frames and reached EOF.
- Every full-file run reported zero underflow callbacks, recycling failures, stream errors, and worker errors.
- An M4A seek to 1.5 seconds used generation 2, rendered 68,608 frames, discarded 16 stale generation-1 blocks, and reported zero underflows or errors.

All hardware smoke tests used gain `0.0` and were therefore silent.

## Current Limits

- The track sample rate must match the active output-device sample rate. The transport returns an explicit error because resampling is not implemented or approved yet.
- This is one deck only. Mixing, cue routing, tempo control, synchronization, and DSP are not part of this stage.
- Track replacement is covered at the decoder-worker boundary; a dedicated hardware replacement smoke test remains useful when the future UI can drive it interactively.
- The transport has not yet completed a multi-hour stability or buffer-size sweep.

