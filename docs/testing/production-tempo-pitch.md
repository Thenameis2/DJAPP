# Production Vinyl Rate

## Implemented Scope

- Per-deck vinyl-style rate from `-16%` through `+16%`.
- Source-position accounting that remains correct when output duration changes.
- Zero added processor latency for neutral and non-neutral vinyl rate.
- Rate restoration after output-device recovery.
- Key lock and independent pitch fail closed with explicit UI explanations.

Sync remains disabled until BPM and beat-grid analysis exists. Pitch bend, jog behavior, key lock, and independent pitch are not part of the accepted production slice.

## Architecture

`TempoProcessor` runs after fixed engine-rate conversion on the existing decoder worker and before bounded PCM blocks reach the callback. Non-neutral manual rate uses stateful linear interpolation across chunk boundaries. Processing and buffer management remain off the CoreAudio callback.

Pitch changes naturally with playback speed, like a turntable. Processed blocks include their source/output ratio so the callback advances the source timeline using only arithmetic and atomics.

Neutral settings (`0%` tempo and `0` semitones) bypass Signalsmith completely. This preserves the decoder/resampler output exactly, reports zero stretch latency, and prevents ordinary playback from inheriting spectral-window artifacts or debug-build processing pressure.

Changing rate during playback updates the existing worker-side varispeed state without seeking the decoder or restarting transport. Already queued audio finishes first, then the new setting takes effect at a block boundary.

Signalsmith remains in the repository for isolated evaluation, but production rate commands do not call it. Real-song testing repeatedly produced looping spectral grains even though controlled fixtures passed. Key lock and independent pitch remain fail-closed while that processing path is reviewed.

## Automated Verification

Run:

```sh
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
```

Processor tests cover approved ranges, varispeed duration, chunk-boundary continuity, reset behavior, and zero latency. Existing transport, mixer, cue, device, decoder, resampler, persistence, and Tauri service tests remain active.

The diagnostic corpus contains an original 20-second music-like WAV plus MP3 and M4A encodings. `tempo_diagnostic` models the 16-block worker queue and 512-frame callback cadence, changes rate to `+8%` and `-8%`, writes rendered WAV output, and rejects highly correlated repeated windows.

The diagnostic is capped at 20 seconds so representative full-length local tracks can be checked without rendering and scanning the entire song. A real 44.1 kHz MP3 exposed a buffer-reuse defect in `EngineRateDecoder`: converted PCM was appended to the returned previous buffer instead of replacing it. Clearing the reusable buffer before copying resampler output removes the repeated sections. A regression now requires fresh-buffer and reused-buffer decoding to produce identical PCM.

The hardware regression changes both deck rates after playback starts, verifies forward source progression, and exercises a seek while both varispeed decks feed the shared CoreAudio stream.

The React rate slider keeps local ownership for the full pointer gesture. Snapshot polling cannot overwrite an active drag, and pointer/key release submits the range element's exact final value. Key lock and pitch controls are visibly disabled.

## Apple M3 Hardware Check

The direct CoreAudio single-output test used the active macOS default output. Deck A changed to `-8%` and Deck B to `+8%` while mixed-rate 48 kHz and 96 kHz fixtures were playing. It then reselected the active output and sought Deck B.

Results:

- 60 shared callbacks.
- Zero clipping or stream errors.
- Zero Deck A or Deck B underflows.
- Zero recycle failures or worker errors.
- No stale Deck A blocks; 16 stale Deck B blocks were correctly discarded after its explicit seek.

The fixture diagnostic reports no repeated windows for WAV, MP3, or M4A. Target-hardware listening on the owner's affected tracks remains the final acceptance step.

## Manual Acceptance

Listen to representative tracks at `-16%`, `-8%`, `+8%`, and `+16%`. Confirm uninterrupted progression and that the expected vinyl-style pitch change and linear-interpolation quality are acceptable. Do not mark key lock or independent pitch accepted.
