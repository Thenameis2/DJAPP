# ADR-012: Track Analysis Pipeline

- Status: Proposed
- Date: 2026-06-15
- Scope: Offline BPM, beat-grid, downbeat, musical-key, waveform, loudness, caching, and analysis scheduling

## Context

The application needs cached track analysis before it can display trustworthy BPM and key values, draw real waveforms, normalize gain, enable beat-aware Sync, or make AutoMix decisions. Analysis must support MP3, WAV, FLAC, AAC/M4A, and AIFF through the existing decoder, remain fully offline, and never interfere with the real-time master or cue paths.

ADR-001 approved the direction of project-owned music-analysis algorithms built on a small permissive Rust foundation. ADR-003 already provides versioned `track_analysis` records and external `beat_grid_path` and `waveform_path` fields, so this milestone does not require a database migration.

## Decision Drivers

1. Stable playback and cueing while analysis runs.
2. Deterministic results on Apple silicon without cloud services.
3. Versioned, invalidatable caches that never modify source music.
4. Confidence-aware results that do not pretend uncertain analysis is exact.
5. Licenses compatible with possible future public distribution.
6. A small dependency and native-build surface.
7. Cancellable work with visible per-track and aggregate progress.

## Proposed Decision

Add only these production dependencies after owner approval:

| Purpose | Choice | License | Reason |
| --- | --- | --- | --- |
| FFT and spectral transforms | `rustfft` | MIT OR Apache-2.0 | Mature pure Rust implementation with AArch64 NEON support |
| Integrated loudness and true peak | `ebur128` | MIT | Focused EBU R128 implementation with established conformance tests |

Reuse existing `symphonia`, `rubato`, and `rusqlite`. Do not add aubio, Essentia, libKeyFinder, SoundTouch, FFmpeg, an async runtime, or a waveform library.

Implement waveform generation, onset extraction, BPM estimation, beat tracking, downbeat confidence, and musical-key estimation in project-owned Rust modules behind typed interfaces. Accuracy must be demonstrated by fixtures and a labeled benchmark before Sync or compatibility-based AutoMix consumes the results.

## Analysis Pipeline

Each job processes one immutable track identity consisting of track ID, path, file size, modification time, content fingerprint when available, and analysis version.

1. Open and decode with the existing `MediaDecoder`.
2. During the decode pass, compute sample peaks, feed EBU R128 loudness, and build the waveform pyramid.
3. Downmix to a normalized mono analysis signal and resample to a fixed 22,050 Hz analysis rate using the existing resampling foundation.
4. Extract spectral-flux onset strength with windowing, adaptive normalization, and silence rejection.
5. Generate tempo candidates with autocorrelation/tempogram scoring over a documented DJ range, initially 60-200 BPM.
6. Resolve half/double-tempo ambiguity using onset periodicity, beat consistency, and genre-neutral priors rather than silently forcing a preferred range.
7. Track beat positions with dynamic programming, then estimate downbeat candidates only when meter evidence is strong enough.
8. Estimate tuning and chroma, reject percussion-dominant or silent windows, correlate major/minor templates, and combine temporal votes into a key and confidence.
9. Atomically write versioned waveform and beat-grid cache files.
10. Commit the final analysis record through the existing persistence worker only after all required artifacts succeed.

All beat and waveform positions use the original source-track frame timeline. Analysis-rate positions must be converted with rational arithmetic so deck seeks and future Sync do not accumulate floating-point drift.

## Result Semantics

- BPM is a positive floating-point value plus confidence from `0.0` to `1.0`.
- Beat grids contain ordered source-frame positions, per-beat strength, and optional downbeat markers.
- Downbeats are optional. Low-confidence meter detection must produce beats without claiming downbeats.
- Musical key uses a canonical major/minor representation internally. Camelot notation is a presentation concern derived in the UI or view model.
- Key confidence and BPM confidence are independent.
- Loudness stores integrated LUFS and true peak dBTP. Tracks too short or too silent for a valid measurement retain an explicit unavailable result.
- Failed analysis stores a concise user-safe error while preserving the source track and any prior user corrections.
- User BPM, key, and beat-grid corrections remain authoritative and separate from generated results.

## Cache Formats

Store large artifacts below the Tauri application cache directory, not inside SQLite.

### Waveform Cache

- Binary little-endian format with magic bytes, format version, analysis version, source sample rate, source channels, source frame count, and level descriptors.
- Level zero stores per-channel minimum, maximum, and RMS values over 256 source frames.
- Coarser levels combine four adjacent buckets until an overview level is available.
- Values use compact fixed-width numeric representations selected by the implementation spike and documented before the format is accepted.

### Beat-Grid Cache

- Binary little-endian format with magic bytes, format version, analysis version, source sample rate, source frame count, BPM, confidence, and ordered beat records.
- Each beat record stores source-frame position, strength, and flags such as downbeat.
- Cache readers validate lengths, ordering, finite values, version, and track identity before exposing data.

Write each artifact to a temporary sibling file, flush it, and rename it atomically. A cancelled, failed, or crashed job must not replace a valid cache with a partial file. Orphan cleanup may remove only application-owned cache files.

## Cache Validity And Versioning

Define one `ANALYSIS_VERSION` and independent waveform/grid format versions in Rust.

A cached result is valid only when:

- its analysis version is current;
- the track is not missing;
- file size and modification time still match;
- content fingerprint matches when one exists;
- both referenced cache files exist and pass header validation.

An algorithm or cache-format change invalidates generated analysis only. It must not delete track corrections, hot cues, saved loops, or source files. Re-analysis replaces cache paths only after successful atomic writes.

## Module Boundaries

- `analysis::service`: bounded job queue, cancellation, progress, deduplication, and shutdown.
- `analysis::pipeline`: coordinates decode and analysis stages without Tauri or SQLite types.
- `analysis::signal`: mono conversion, fixed-rate conversion, windows, normalization, and reusable scratch buffers.
- `analysis::rhythm`: onset envelope, tempo candidates, beat tracking, downbeat confidence, and rhythm diagnostics.
- `analysis::key`: tuning, chroma extraction, template scoring, voting, and key confidence.
- `analysis::waveform`: peak-pyramid construction and validated cache codec.
- `analysis::loudness`: `ebur128` adapter and sample/true-peak results.
- `analysis::cache`: application-owned paths, identity validation, atomic writes, and cleanup.
- `persistence`: stores status and compact results through the existing worker; it does not perform analysis.
- `src-tauri`: translates commands and progress events; it does not own algorithm state.
- React: displays status, progress, BPM, key, and cached waveform data without receiving full decoded PCM.

## Scheduling And Thread Safety

- Start with one background analysis worker. This gives predictable CPU and memory use while audio is active; concurrency can be benchmarked later.
- Use a bounded queue and deduplicate by track ID plus track identity.
- Prioritize an explicitly requested or newly loaded track over passive library backlog.
- Cancellation uses atomics or non-blocking control messages checked between bounded processing blocks.
- Decoding, FFT work, cache I/O, allocation, and SQLite requests are forbidden in CPAL callbacks.
- Analysis may share immutable algorithm configuration, but mutable per-track state stays worker-local.
- Playback never waits for analysis. Missing analysis disables dependent features and shows a clear state.
- Shutdown stops accepting work, cancels queued jobs, lets the active job reach a safe checkpoint, and joins the worker.

## Progress And API Shape

Expose typed internal commands for:

- analyze one track;
- analyze all stale or missing tracks;
- cancel one track;
- cancel pending library analysis;
- query current jobs and aggregate progress;
- load validated waveform or beat-grid windows.

Progress stages are `queued`, `decoding`, `waveform`, `rhythm`, `key`, `loudness`, `writing`, `complete`, `failed`, and `cancelled`. Progress is monotonic within a job and throttled before being emitted to React. Exact Tauri command and event names are implementation details, but payloads must include track ID, stage, completed fraction when known, and a user-safe message.

## Confidence And Feature Gating

- The library may display low-confidence BPM and key with an uncertainty indicator.
- Beat Sync remains disabled unless BPM and beat-grid confidence meet benchmarked thresholds and the grid covers the current deck position.
- Downbeat-dependent transitions remain disabled when downbeats are unavailable.
- AutoMix may use uncertain values only as weak ranking signals and must tolerate missing analysis.
- Manual correction always overrides generated BPM, key, and grid offset without changing the cached source analysis.

Initial thresholds for evaluation are `0.65` for BPM/grid use by Sync and `0.60` for displaying a key without an uncertainty marker. These are provisional and must be calibrated from the benchmark corpus rather than treated as universal constants.

## Testing Strategy

### Deterministic Unit Tests

- Mono/stereo downmix, silence handling, window overlap, and source-frame conversion.
- FFT magnitudes and spectral-flux behavior on synthetic impulses, clicks, sweeps, tones, and noise.
- Tempo candidates at 60, 90, 120, 128, 150, and 180 BPM, including syncopation and half/double-tempo cases.
- Beat-grid continuity, missing-onset tolerance, and no non-monotonic beat positions.
- Major/minor key fixtures across multiple octaves, tuning offsets, silence, and percussion-heavy input.
- Waveform extrema/RMS preservation across pyramid levels and cache corruption rejection.
- Loudness and true-peak checks against published or dependency-provided conformance fixtures where redistribution permits.
- Cancellation, deduplication, stale identity, atomic replacement, and shutdown behavior.

### Integration Tests

- Analyze all supported formats and require equivalent results within documented tolerances.
- Persist `pending`, `running`, `complete`, and `failed` states through the existing schema.
- Reopen valid caches without decoding the track again.
- Invalidate changed files and analysis versions while preserving corrections, cues, and loops.
- Run analysis while two decks feed master and cue; callback underflow and stream-error counters must not regress.

### Quality Benchmark

Maintain redistributable synthetic fixtures in the repository and a documented local benchmark manifest for licensed/private reference tracks that are never committed.

Before analysis is accepted for Sync:

- synthetic steady-tempo BPM error is at most `0.1%`;
- labeled music BPM is within `1%` or a documented half/double equivalent for at least `90%` of the benchmark set;
- steady 4/4 beat-grid median absolute timing error is at most `20 ms` on accepted high-confidence tracks;
- key accuracy is reported as exact, relative/parallel, neighboring Camelot, and incorrect rather than reduced to one misleading number;
- confidence calibration demonstrates that rejected low-confidence tracks fail more often than accepted tracks;
- a full-library soak completes with bounded memory, no source-file changes, and no playback underflows attributable to analysis.

If custom rhythm or key analysis misses these targets, stop before enabling Sync or AutoMix ranking and present a new dependency/licensing ADR rather than lowering the claims.

## Incremental Implementation Plan

1. Add approved `rustfft` and `ebur128` versions, project-owned analysis result types, cache headers, and synthetic fixtures.
2. Implement the bounded analysis service, cancellation, identity checks, and persistence status transitions.
3. Implement waveform pyramid and loudness/peak analysis, then integrate cached waveform display.
4. Implement fixed-rate signal preparation, onset envelope, BPM candidates, and benchmark reporting.
5. Implement beat tracking and optional downbeat confidence, then persist the beat-grid cache.
6. Implement key estimation and manual-correction precedence.
7. Add Tauri progress/query commands and analysis states to the existing library UI without redesigning the screen.
8. Run Apple M3 playback-under-analysis, cache-reopen, cancellation, and local quality benchmarks.
9. Present benchmark results for acceptance before enabling master/follower Sync or analysis-driven AutoMix.

## Tradeoffs

Project-owned MIR algorithms keep licensing and packaging simple but require substantial validation and may initially be less accurate than GPL/AGPL toolkits. One worker favors audio reliability over fastest library scans. External binary caches add format/version maintenance, but they avoid SQLite contention and allow the UI to request only visible waveform or grid regions.

This ADR intentionally separates producing trustworthy analysis from consuming it. BPM display and waveform rendering can arrive before Sync, while uncertain beat grids or keys remain visibly limited.

## Approval Request

Approval authorizes:

- the pipeline, module boundaries, cache strategy, confidence policy, tests, and incremental plan in this ADR;
- adding pinned production versions of `rustfft` and `ebur128` after confirming their current compatible releases;
- using the existing schema without migration;
- implementing analysis status/progress in the established library UI without changing its layout or visual language.

Approval does not authorize a schema migration, additional analysis dependencies, automatic BPM/key correction, enabling Sync before benchmark acceptance, analysis-driven AutoMix, a UI redesign, cloud analysis, telemetry, or modification of source music files.
