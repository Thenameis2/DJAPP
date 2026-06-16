# ADR-012: Track Analysis Pipeline

- Status: Approved; stages one through nine implemented
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
| FFT and spectral transforms | `rustfft` 6.4.1 | MIT OR Apache-2.0 | Mature pure Rust implementation with AArch64 NEON support |
| Integrated loudness and true peak | `ebur128` 0.1.10 | MIT | Focused EBU R128 implementation with established conformance tests |

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

## Approval And Stage-One Result

The owner approved ADR-012 on 2026-06-15.

Stage one is implemented:

- Added direct pinned dependencies on `rustfft` 6.4.1 and `ebur128` 0.1.10. RustFFT was already present transitively through the resampling stack.
- Added project-owned analysis stages, track identities, confidence-validated estimates, musical-key types, and aggregate result types.
- Defined `ANALYSIS_VERSION`, waveform format version 1, and beat-grid format version 1.
- Added strict little-endian waveform and beat-grid codecs with identity digests, source-timeline metadata, finite-value checks, monotonic beat validation, version enforcement, and malformed-length rejection.
- Added deterministic 22,050 Hz click-track and major-triad fixtures for upcoming rhythm and key tests.

No worker, persistence flow, waveform computation, loudness computation, BPM detection, beat tracking, key estimation, Tauri API, or UI behavior was added in stage one.

## Stage-Two Result

Stage two is implemented:

- Added one `djapp-analysis` worker with a bounded 64-job default queue and an injectable `AnalysisProcessor` boundary for later pipeline stages.
- Added exact-identity deduplication. Re-enqueuing identical work is rejected, while a changed file identity replaces stale queued work and cooperatively cancels stale active work for the same track.
- Added cooperative cancellation tokens, queued-job removal, retained job snapshots, queue-full errors, clean shutdown signaling, and worker joining.
- Added persistence transitions through the existing single SQLite owner: `pending` on enqueue, `running` before processing, and `complete` or `failed` after processing.
- Added an analysis-record query to the existing persistence worker for status and test inspection. No schema or migration changed.
- Added result validation before persistence for analysis version, BPM, confidence, key, loudness, and true-peak values.

Schema version 1 does not include `cancelled` in the `track_analysis.status` constraint. Runtime snapshots therefore use the distinct `cancelled` stage, while durable cancellation is represented as `failed` with the exact error message `analysis cancelled`. Changing that durable representation requires a separately approved schema migration.

The service is not started by Tauri yet. Stage three will provide the first real processor implementation for waveform and loudness analysis before application lifecycle or UI integration is added.

## Stage-Three Result

Stage three is implemented:

- Added `WaveformLoudnessProcessor` behind the stage-two `AnalysisProcessor` boundary.
- Decodes each track once while feeding EBU R128 integrated loudness/true-peak analysis and a project-owned waveform builder.
- Produces per-channel minimum, maximum, and RMS buckets at 256 source frames, then combines groups of four into progressively coarser levels until one overview bucket remains.
- Finalized waveform format version 1 values as little-endian `f32` triples ordered per channel as minimum, maximum, and RMS. Coarse RMS values are weighted by actual contributing frame counts.
- Validates file size and modification time before and after decoding so changed source files are not cached under a stale identity.
- Writes waveform caches through a temporary sibling, `sync_all`, and atomic rename. Cancellation and failures do not publish a waveform path.
- Persists integrated LUFS, true peak dBTP, and the waveform path through the existing analysis and persistence workers.
- Verified MP3, WAV, FLAC, AAC/M4A, and AIFF fixtures through the complete processor.

Detailed behavior and checks are recorded in `docs/testing/waveform-and-loudness-analysis.md`.

The analysis service remains disconnected from Tauri application lifecycle and React. That integration stays in the approved later UI/API stage so the next algorithm milestone can build BPM estimation on the same single-pass pipeline without exposing an unstable contract.

## Stage-Four Result

Stage four is implemented:

- Added deterministic mono conversion to a fixed 22,050 Hz analysis rate.
- Added streaming 2,048-sample Hann FFT analysis with 512-sample hops using `rustfft` 6.4.1.
- Added positive spectral flux, adaptive local normalization, silence/low-evidence rejection, and a compact onset envelope.
- Added normalized autocorrelation candidates over the strict 60-200 BPM range, harmonic-neighborhood half/double-tempo scoring, parabolic lag interpolation, and long-span onset-peak regression.
- Added BPM confidence and ranked diagnostics while persisting only the selected BPM and confidence through the existing result contract.
- Added `bpm_benchmark`, which reports deterministic synthetic accuracy and can inspect a supplied local track without persistence.
- Refactored fixed-rate and FFT processing to retain decoder-chunk output, FFT overlap, and onset data rather than full-track PCM.

The six-tempo synthetic benchmark has a worst error of 0.009645%, below the 0.1% target. The original music-like fixture measures 119.811985 BPM for its intended 120 BPM pulse. Accented and syncopated fixtures resolve to 120 BPM, while silence and steady tones do not claim a tempo.

Detailed results are recorded in `docs/testing/bpm-analysis.md`.

BPM and beat grids remain analysis metadata only. Sync, beat-aware loops, and AutoMix timing stay disabled pending corpus validation and the later acceptance milestone.

## Stage-Five Result

Stage five is implemented:

- Added dynamic-programming onset linking near the selected tempo, weighted beat-line fitting, and full-track grid generation.
- Added per-beat onset strength and confidence combining BPM evidence, tracked-onset coverage, and grid strength.
- Added conservative four-beat accent scoring. Downbeat flags are written only when one meter phase has sufficiently distinct evidence; otherwise the grid contains beats without downbeats.
- Added nearest-frame rational conversion from the 22,050 Hz analysis timeline to original source frames without iterative floating-point accumulation.
- Added atomic beat-grid cache writes and persistence through the existing schema version 1 `beat_grid_path` field.
- Added `beat_grid_benchmark` and cache/persistence regression coverage.

Across deterministic 60, 90, 120, 128, 150, and 180 BPM click fixtures, median absolute timing error ranges from 9.524 ms to 11.882 ms. Accented and syncopated 120 BPM fixtures measure 10.612 ms. These results satisfy the ADR's 20 ms steady-4/4 median target. Only the clearly accented fixture receives downbeat markers.

Detailed results and limitations are recorded in `docs/testing/beat-grid-analysis.md`.

Sync remains disabled. The deterministic benchmark is necessary but does not replace validation against a labeled music corpus, variable-tempo material, and playback-under-analysis testing.

## Stage-Six Result

Stage six is implemented:

- Added streaming 8,192-sample Hann FFT key analysis with 4,096-sample hops over the existing 22,050 Hz fixed-rate signal.
- Added local spectral-peak chroma, tuning-offset estimation, normalized temporal aggregation, low-energy rejection, harmonic-support gating, and broadband-uniformity rejection.
- Added rotated major/minor tonal-template scoring for all 24 canonical keys with confidence derived independently from BPM confidence.
- Added canonical `pitch-class:major|minor` persistence and validation.
- Added typed correction writes and an effective-analysis read model that applies user BPM, key, and beat-grid-offset corrections without modifying generated analysis.
- Added `key_benchmark` with exact, relative/parallel, neighboring-Camelot, and incorrect reporting categories.

All 24 synthetic major/minor triads are classified exactly. A-minor fixtures remain correct from -35 through +35 cents. Silence, broadband noise, and pitched-percussion clicks do not claim a key.

Detailed results and limitations are recorded in `docs/testing/musical-key-analysis.md`.

Generated BPM, beat grids, and key remain analysis metadata. Stage seven must expose progress, status, validated artifacts, and correction precedence through Tauri and the established library UI before any Sync or analysis-driven AutoMix acceptance decision.

## Stage-Seven Result

Stage seven is implemented:

- Tauri now starts and owns one bounded analysis worker using application-data SQLite and application-cache artifacts.
- Added typed commands for one-track and stale-library analysis, cancellation, job snapshots, correction writes, and identity-validated waveform/beat-grid reads.
- Added source-identity stale detection so a complete database status does not hide a changed file, missing artifact, corrupt waveform cache, or old analysis version.
- Added stage-level progress reporting through decoding, waveform, rhythm, key, loudness, writing, and completion.
- Enriched library rows with effective BPM/key values, confidence, correction flags, analysis status/errors, and artifact availability.
- Updated the existing library table with Analyze, Cancel, Analyze Missing, and Correct controls plus uncertainty and corrected-value indicators. The deck/mixer layout was not redesigned.

The integration contract and manual acceptance checklist are recorded in `docs/testing/analysis-ui-integration.md`.

Sync and analysis-driven AutoMix remain disabled. Stage eight must run Apple M3 playback-under-analysis, cache-reopen, cancellation, and local quality benchmarks before presenting final feature-gating recommendations.

## Stage-Eight Result

Stage eight is implemented:

- Added a direct CoreAudio acceptance harness that runs full analysis while two muted, mixed-sample-rate decks play on the Apple M3 target.
- Added completed-cache reopen coverage through a new persistence worker after removing the source fixture, proving cached results do not require another decode.
- Added active production-pipeline cancellation coverage proving no partial waveform or beat-grid artifact is published.
- Re-ran the BPM, beat-grid, and key diagnostics and the complete supported-format analysis suite.

The Apple M3 run advanced from 43 to 219 callbacks during analysis with zero mixer stream errors, deck underflows, recycle failures, or worker errors. Deterministic BPM, grid, and key targets continue to pass. Detailed commands and results are recorded in `docs/testing/analysis-acceptance.md`.

Engineering acceptance passes for the available deterministic corpus and target-hardware stability. A labeled private/licensed music corpus and representative full-library soak were not available, so real-music accuracy and confidence calibration remain unmeasured.

## Stage-Nine Result

Stage nine is implemented as a feature-gating decision, not as a new analysis algorithm.

Accepted for continued use:

- Offline analysis scheduling, cancellation, cache publication, stale-cache rejection, and queue visibility.
- Library display of generated BPM, key, waveform availability, beat-grid availability, confidence, uncertainty markers, and user corrections.
- Manual BPM/key correction precedence over generated values.
- Developer diagnostics and synthetic benchmarks for regression detection.
- Playback-under-analysis on the Apple M3 target for the tested muted, mixed-rate scenario.

Not accepted for consumption yet:

- Master/follower Sync.
- Analysis-driven AutoMix timing or compatibility reordering.
- Downbeat-dependent transitions.
- Automatic correction of BPM, key, or beat grids.
- Treating generated BPM as authoritative when the owner's comparison software disagrees.

Decision:

Keep Sync and analysis-driven AutoMix disabled. Generated BPM/key may remain visible in the library with uncertainty markers and manual correction controls, but the engine must not use generated BPM or beat-grid data for live synchronization until real-music accuracy is measured and accepted.

Reason:

The deterministic fixtures and Apple M3 playback-under-analysis acceptance prove stability, cache integrity, and baseline algorithm behavior. They do not prove DJ-grade real-library accuracy. Owner feedback after Stage 8 found generated BPM values that are inaccurate compared with other DJ software, and one real-track rhythm-analysis boundary panic was fixed afterward. That panic fix improves safety, not accuracy.

Required next milestone before Sync or analysis-driven AutoMix:

1. Build a private local benchmark manifest with representative tracks and expected BPM values from the owner's comparison software. Do not commit copyrighted audio.
2. Include easy, hard, and known-failing songs, with half-time and double-time expectations noted explicitly.
3. Run `bpm_benchmark` and beat-grid diagnostics against the manifest.
4. Tune the BPM estimator, ambiguity handling, and confidence thresholds so bad results are flagged rather than silently trusted.
5. Re-run the full analysis acceptance suite and a representative full-library soak.
6. Present a new acceptance report. If the project-owned algorithm cannot meet the ADR targets, prepare a dependency/licensing ADR instead of weakening the Sync requirements.

Detailed Stage 9 recommendations are recorded in `docs/testing/analysis-feature-gating.md`.
