# Analysis UI Integration

## Application Lifecycle

Tauri starts one `AnalysisService` beside the existing persistence and mixer services. The worker uses the application cache directory for waveform and beat-grid artifacts and the existing application-data SQLite database for compact results and job status. Dropping Tauri state cooperatively cancels queued/active analysis and joins the worker.

Analysis remains outside CPAL callbacks and the React thread. The UI polls compact job snapshots once per second and refreshes library rows only while work is active.

## Tauri Commands

- `analysis_analyze_track(trackId)` rebuilds current filesystem identity and enqueues one track.
- `analysis_analyze_all()` queues tracks with missing, failed, old-version, missing-cache, corrupt-cache, or mismatched-identity analysis.
- `analysis_cancel_track(trackId)` cooperatively cancels queued or active work.
- `analysis_jobs()` returns ordered job snapshots with stage, optional fraction, and user-safe message.
- `analysis_set_correction(...)` validates and stores manual BPM, canonical key, and grid-offset corrections separately from generated analysis.
- `analysis_artifacts(trackId)` decodes waveform and beat-grid caches, verifies their identity digest against the current file, and returns the coarsest waveform overview plus beat records.
- `library_tracks()` now returns effective BPM/key values, confidence, correction flags, analysis status/error, and artifact availability.

Progress stages advance through decoding, waveform, rhythm, key, loudness, writing, and complete. Fractions are stage-level estimates rather than byte-accurate decode progress.

## Existing-Layout UI

The music-library table now includes BPM, key, and analysis columns. It provides:

- per-track Analyze and Cancel controls;
- an Analyze Missing action for stale or incomplete library entries;
- an Analyze Queue popup that opens automatically when one or more jobs are active or waiting;
- queue order, current stage, progress percentage, generated BPM/confidence, and cancellation controls per queued song;
- uncertainty markers below provisional confidence thresholds;
- corrected-value highlighting;
- a compact correction flow accepting BPM and common key labels such as `C`, `F#`, or `Am`;
- live analysis stage and failure indication.

The deck, mixer, routing, and AutoMix layouts are unchanged. Sync and analysis-driven AutoMix remain disabled.

## Verification

Automated checks cover the engine analysis pipeline, service progress contract, correction precedence, cache codecs, and Tauri stale-identity detection. The frontend TypeScript and production Vite build validate the command payload shapes used by React.

Manual desktop acceptance should confirm that a scanned track can be analyzed, shows intermediate stages, receives BPM/key values, accepts and clears corrections, survives application restart, and rejects artifacts after the source file changes.

## BPM Quality Follow-Up

The current BPM estimator is deterministic and cached, but owner feedback shows it is not yet accurate enough against commercial DJ software on real music. Do not enable Sync or analysis-driven AutoMix from generated BPM until a labeled local corpus is collected and benchmarked.

The next quality pass should use representative tracks with expected BPM values from comparison software, include half-time/double-time cases, and tune confidence thresholds so uncertain results are clearly flagged instead of presented as reliable.
