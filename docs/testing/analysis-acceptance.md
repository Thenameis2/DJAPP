# Track-Analysis Acceptance

Date: 2026-06-15
Target: Apple M3 macOS system

## Playback Under Analysis

The ignored CoreAudio acceptance test opens the current default output, loads 48 kHz and 96 kHz fixtures into the two-deck mixer, mutes both channels, starts both decks, and runs the complete waveform, loudness, BPM, beat-grid, and key pipeline against the 20-second music-like fixture.

Command:

```sh
cargo test --offline --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::two_deck_playback_remains_healthy_during_full_track_analysis \
  -- --ignored --nocapture
```

Measured Apple M3 result:

- Callbacks before analysis: 43.
- Callbacks after analysis: 219.
- Mixer stream errors: 0.
- Deck A underflows: 0.
- Deck B underflows: 0.
- Deck A recycle failures: 0.
- Deck B recycle failures: 0.
- Deck worker errors: none.
- Analysis status: complete.

Result: the muted two-deck CoreAudio stream continued advancing during full analysis with no health-counter regression.

## Cache Reopen And Cancellation

Automated integration coverage verifies:

- a completed SQLite record can be reopened in a new persistence worker;
- waveform and beat-grid caches decode successfully after the source fixture is removed, proving reopen does not require another media decode;
- waveform and beat-grid identity digests agree;
- active production analysis can be cancelled during waveform/decode work;
- cancellation persists the schema-version-1 representation `failed` with `analysis cancelled`;
- cancellation publishes neither waveform nor beat-grid paths and leaves no cache directory.

## Quality Benchmarks

Current deterministic results remain:

- BPM: worst steady-tempo error `0.009645%`, below the `0.1%` target.
- Beat grid: median absolute timing error from `9.524 ms` through `11.882 ms`, below the `20 ms` target.
- Key: 24 of 24 synthetic major/minor fixtures classified exactly.
- Tuning: A minor remains correctly classified from `-35` through `+35` cents.
- Rejection: silence, stationary tones, broadband noise, and pitched percussion do not claim unsupported analysis values.
- Supported formats: MP3, WAV, FLAC, AAC/M4A, and AIFF complete waveform/loudness processing through the shared decoder path.

## Acceptance Boundary

Stage-eight engineering acceptance passes for deterministic quality, cache reopen, production cancellation, and Apple M3 playback-under-analysis stability.

This does not yet satisfy final Sync or compatibility-based AutoMix acceptance. No labeled private/licensed music corpus was provided, so the ADR targets for at least 90% labeled-music BPM accuracy, real-music key classification, confidence calibration, and a representative full-library soak remain unmeasured. Stage nine records this limitation explicitly and keeps dependent features disabled unless the owner supplies an appropriate local benchmark corpus or approves a revised validation plan.
