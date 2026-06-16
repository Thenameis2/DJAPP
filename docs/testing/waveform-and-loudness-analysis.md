# Waveform And Loudness Analysis

## Implemented Scope

- Single-pass decoding through the existing Symphonia boundary.
- Per-channel waveform buckets containing minimum, maximum, and RMS values.
- A pyramid beginning at 256 source frames per bucket and combining four adjacent buckets per level.
- EBU R128 integrated loudness and true peak through `ebur128` 0.1.10.
- Source-identity validation before and after decoding.
- Atomic versioned waveform cache writes beneath an application-owned cache root.
- Persistence of integrated LUFS, true peak dBTP, and waveform path through the stage-two analysis service.

BPM, beat grids, downbeats, musical key, Tauri lifecycle integration, and React waveform rendering are not part of this stage.

## Cache Representation

Waveform cache format version 1 stores little-endian `f32` values. For every bucket, each channel contributes three values in this order:

1. Minimum sample.
2. Maximum sample.
3. Root-mean-square sample level.

Coarser RMS values are weighted by their contributing source-frame counts, including partial final buckets. Cache headers include the analysis version, source sample rate, channel count, source frame count, and a deterministic 32-byte track-identity digest.

The processor writes a uniquely named temporary sibling, flushes it with `sync_all`, and renames it to the final cache path. Failed or cancelled work removes temporary files and does not publish a cache path.

## Verification

Automated tests cover:

- extrema and weighted RMS preservation across pyramid levels;
- stereo channel ordering;
- malformed and non-finite PCM rejection;
- cache encode/decode validation;
- cancellation before cache creation;
- changed source identity rejection;
- MP3, WAV, FLAC, AAC/M4A, and AIFF analysis;
- finite loudness and true-peak results for the tone fixtures;
- end-to-end service and SQLite persistence of waveform/loudness results.

Run:

```sh
cargo test --offline --all-targets
cargo clippy --offline --all-targets -- -D warnings
cargo test --offline --manifest-path src-tauri/Cargo.toml
cargo clippy --offline --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
```

Current result: 64 engine tests, six CLI tests, and two default Tauri tests pass. Two hardware-dependent CoreAudio tests remain ignored unless the required output devices are available.
