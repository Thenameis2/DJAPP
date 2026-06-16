# Project Memory

## How To Use This File

Read this file before every task and update it after every meaningful change. Keep confirmed decisions, current state, completed work, bugs, schema changes, API contracts, and future work accurate. Mark replaced decisions as superseded instead of deleting them.

## Current State

- Status: Reliable streaming vinyl-style rate adjustment is implemented without Signalsmith. ADR-012 stages one through nine are implemented. Analysis is accepted for library metadata, cache reuse, queue visibility, manual corrections, and diagnostics. Analysis data is not consumed by Sync or AutoMix because generated BPM accuracy remains below the owner benchmark gate.
- Target: private, offline-first macOS DJ desktop application.
- Target hardware: Apple M3 Mac.
- Implementation: reusable Rust engine plus a Tauri 2, React, TypeScript, and Vite macOS application scaffold.
- Production architecture: approved.
- Database schema: version 1 created and tested.
- External APIs: none.
- Known bugs: Physical channels 3–4 cue output has not yet been tested because no four-channel device or aggregate device is currently available. Analysis is exposed in the library UI but Sync and AutoMix do not consume it. Target-hardware playback-under-analysis passes, but key/rhythm accuracy, confidence calibration, and full-library behavior still require a labeled representative music corpus.
- Current analysis issue: owner reports generated BPM is inaccurate compared with other DJ software on real music. Queue visibility has been improved, and the first five-track private benchmark now accepts 4/5 exact BPMs after precision refinement, but BPM quality remains unresolved because the ADR gate requires at least 90% acceptance on a larger representative set.

## Confirmed Product Decisions

### 2026-06-13 - Product Direction

- The primary user is a DJ.
- The application must be fully usable without an external DJ controller.
- Version 1 targets macOS and Apple silicon, initially an Apple M3 machine.
- The app works offline with local music files selected from recursive folder trees.
- No user accounts, cloud backend, subscriptions, streaming services, payments, bookings, messaging, requests, or live social features are required.
- The app has two virtual decks and professional DJ controls.
- Required analysis includes BPM, beat grid, musical key, waveform, and loudness, cached locally.
- AutoMix supports preserving queue order and compatibility-based reordering, selected by the user when AutoMix starts.
- Completed mix recording is not required for version 1.
- Supported music formats should include MP3, WAV, FLAC, AAC/M4A, and AIFF where codec support permits.
- Built-in, wired, Bluetooth, and compatible external output devices should be supported when exposed by macOS.
- Headphone cue should support deck A, deck B, and cue/master blend when available audio routing makes this possible.
- Settings and analysis data may be stored locally.
- Tests and documentation must be updated after every meaningful change.
- Persistent memory records decisions, completed work, bugs, schema changes, API contracts, and future tasks.
- Owner approval is required before architecture, dependency, database-schema, or established UI-design changes.
- The application is currently for private use but may later be distributed publicly.
- The interface must provide both light and dark themes and save the user's selection locally.

## Approved Architecture

### 2026-06-13 - Initial Architecture Approved

- Tauri desktop shell.
- React and TypeScript interface.
- Rust audio engine, analysis, and device integration.
- SQLite local persistence.
- No hosted deployment for version 1.

Status: approved by the owner.

Reasoning: a desktop application is better suited than a browser application for offline local-file access, low-latency audio processing, and macOS audio-device routing.

## Constraints And Technical Facts

### 2026-06-13 - Audio Routing

- Bluetooth output normally adds enough latency to impair live beat mixing. It is supported as a convenience, not the recommended performance path.
- Independent stereo master and headphone cue require separate addressable channels or devices. A Mac's built-in output alone may not provide both after headphones are connected.
- Possible future solutions include a multi-output audio interface, controller with audio outputs, macOS aggregate devices, or a mono split-output mode. No solution has been selected.
- With wired headphones connected on 2026-06-14, CoreAudio exposes External Headphones and MacBook Pro Speakers as separate 2-channel 44.1 kHz output devices. This enables a dual-stream routing experiment but does not prove shared-clock synchronization or matched latency.

## Completed Work

### 2026-06-13 - Requirements Documentation

- Created `AGENTS.md` with developer workflow, approval gates, engineering standards, testing expectations, and persistent-memory rules.
- Created `REQUIREMENTS.md` with product scope, functional and non-functional requirements, version-one acceptance criteria, and open decisions.
- Created this persistent project memory.

### 2026-06-13 - Architecture And Themes Confirmed

- Type: decision
- Status: approved
- Change: Approved Tauri, React with TypeScript, Rust, and SQLite as the application architecture. Confirmed both light and dark themes, with dark as the default and the selected theme saved locally.
- Reason: The architecture supports the required offline desktop and audio workflow; both themes are required by the owner.
- Verification: Updated `AGENTS.md`, `REQUIREMENTS.md`, and `MEMORY.md` consistently.
- Follow-up: Evaluate and obtain approval for specific production dependencies.

## Database And API History

- SQLite schema version 1 is embedded in `src/persistence.rs` and tracked with `PRAGMA user_version`.
- A single-owner persistence worker provides typed settings, track-upsert, and queue commands. Direct database access is confined to the persistence module.
- Internal Tauri commands include library scanning/query; output-device discovery and selection; Deck A and Deck B load/play/pause/seek/stop; per-deck tempo, key lock, pitch, gain, and cue selection; `mixer_snapshot`; crossfader; master gain; cue gain; and cue/master blend. Deck transport commands return the combined mixer snapshot. No external API contracts exist.

## Known Issues And Risks

- Target machine runs macOS 26.4.1 on arm64 Apple M3 hardware.
- CPAL 0.18.1, `rtrb` 0.3.4, Symphonia 0.6.0, Rubato 3.0.0, `rusqlite` 0.40.1 with bundled SQLite, and `signalsmith-stretch` 0.1.3 are approved and locked production dependencies.
- The project minimum toolchain is Rust 1.96 because the approved bundled SQLite build does not compile on the previous Rust 1.85 toolchain.
- Exact effects for version 1 have not been selected.
- Signalsmith's default preset reports 120 ms processor latency and has ample two-deck release CPU headroom on the Apple M3. Whole-application latency and remaining DSP performance targets still require measurement.
- Headphone cue without separate audio hardware may be limited by macOS device capabilities.
- CPAL's CoreAudio backend may report `unknown` interface type for some Bluetooth devices, so the UI displays wireless latency guidance beside all output selections.
- Public distribution would introduce signing, notarization, licensing, privacy, and support requirements.
- Sample-rate conversion allocates temporary output/adapter buffers on decoder workers; allocation reuse remains a performance optimization.
- The temporary application icon and scaffold layout are not approved final branding or production UI design.
- The theme toggle is session-only until a settings command is connected to SQLite.
- Scans skip symlinks. If traversal errors occur, missing-file reconciliation is skipped for that scan to prevent false missing records.
- One dedicated mixer-service thread owns both decks and the single CoreAudio stream; Tauri commands never hold or share the stream directly. UI snapshots poll at 2 Hz.
- Generated BPM is not yet trusted for Sync or AutoMix because owner comparison against other DJ software found inaccurate values. The next analysis-quality pass needs representative local tracks, expected BPM labels, and half-time/double-time review.

## Next Meaningful Tasks

1. Complete the Signalsmith listening check on percussion, vocals, bass-heavy music, and sustained harmonic material.
2. Design and approve momentary pitch-bend and playing/paused jog behavior on top of the production tempo worker.
3. Populate the private BPM accuracy benchmark manifest with owner-selected tracks and expected BPM values, then tune the rhythm estimator before enabling Sync or analysis-driven AutoMix.

### 2026-06-15 - Analysis Queue UX And BPM Accuracy Follow-Up

- Type: bug
- Status: partially mitigated
- Change: Added queue-position reporting to analysis job snapshots, changed `analysis_analyze_all()` to return a structured enqueue summary, and added a React Analyze Queue popup that auto-opens while analysis work is active. The popup shows order, current stage, progress, generated BPM/confidence, and per-track cancellation.
- Reason: The owner reported that long analysis queues felt slow and unresponsive, and that generated BPM values are inaccurate compared with other DJ software.
- Verification: Engine tests, Tauri tests, strict Clippy runs, frontend production build, formatting, and diff checks passed during the queue UI update.
- Follow-up: Collect a small representative set of failing tracks with expected BPM values from the comparison software, then tune the BPM estimator and confidence thresholds before enabling Sync or analysis-driven AutoMix.

### 2026-06-15 - Rhythm Analyzer Boundary Panic Fixed

- Type: bug
- Status: completed
- Change: Fixed an out-of-range slice panic in `sample_envelope()` when beat-grid fitting requests strength just past the last onset-envelope bucket.
- Reason: The owner hit a `djapp-analysis` panic at `src/analysis/rhythm.rs:301` while analyzing real music. A fitted beat near the source tail could produce an envelope position beyond the slice length.
- Verification: Added a regression test for out-of-range envelope sampling. `cargo test --offline analysis::rhythm`, `cargo test --offline --all-targets`, `cargo clippy --offline --all-targets -- -D warnings`, and `cargo fmt --all --check` pass.
- Follow-up: Continue BPM accuracy work separately with labeled real tracks; this fix prevents the panic but does not claim to improve BPM correctness.

### 2026-06-13 - Dependency Architecture Evaluation

- Type: decision
- Status: proposed
- Change: Created `docs/architecture/ADR-001-audio-analysis-and-persistence-dependencies.md`. It recommends staged evaluation of CPAL, Symphonia, `rtrb`, `rusqlite`, `ebur128`, RustFFT, and conditionally Signalsmith Stretch. It recommends project-owned waveform, BPM, beat-grid, downbeat, and key analysis code, validated against a quality corpus.
- Reason: The selected foundation is small, macOS-compatible, mostly pure Rust, and permissively licensed. Mature ready-made beat and key libraries reviewed are primarily GPL or AGPL, while the Signalsmith Rust wrapper needs a focused stability and quality spike.
- Verification: Compared primary project documentation, repository status, platform support, licenses, performance characteristics, maturity, and Tauri integration implications. No dependencies were installed and no application was scaffolded.
- Follow-up: Owner review of ADR-001, followed by separate approval for CPAL and `rtrb` if the direction is accepted.

### 2026-06-13 - ADR-001 Approved

- Type: decision
- Status: approved
- Change: The owner approved ADR-001's dependency recommendations, module boundaries, audio-thread safety rules, testing strategy, and staged implementation order.
- Reason: The proposed approach provides a small, dependable, mostly permissively licensed foundation while isolating higher-risk analysis and time-stretching work behind prototypes.
- Verification: Updated ADR-001 status and approval section, and reconciled the current state and next tasks in `MEMORY.md`.
- Follow-up: Request explicit approval to add CPAL and `rtrb` for the first audio-device and callback stability spike.

### 2026-06-13 - Stage-One Audio Spike Completed

- Type: completed
- Status: completed
- Change: Approved and locked CPAL 0.18.1 and `rtrb` 0.3.4. Added a minimal Rust CLI spike that enumerates CoreAudio devices, opens the default output, renders silence or an optional bounded-gain tone, receives gain and stop commands through a fixed-capacity SPSC queue, and reports callback health counters. Added a manual test guide.
- Reason: Validate Apple-silicon device access, CPAL compatibility, real-time queue behavior, and the basic callback lifecycle before building the application shell or decoder pipeline.
- Verification: `cargo fmt --check`, five unit tests, clippy with warnings denied, and the release build passed. On macOS 26.4.1, CoreAudio found MacBook Pro Speakers as a two-channel 44.1 kHz `f32` default output with a reported 14-4096 frame buffer range. A three-second silent run completed 260 callbacks and 266,240 interleaved samples, acknowledged two commands, produced zero stream errors, and stopped cleanly.
- Follow-up: Request approval for Symphonia and its selected format features. Longer duration, buffer-size, underrun, CPU, and latency benchmarks remain future work.

### 2026-06-13 - Audio Runtime Environment Constraint

- Type: technical debt
- Status: unresolved
- Change: Documented that the managed filesystem sandbox does not expose CoreAudio devices; the compiled probe must run with direct device access for hardware tests.
- Reason: The sandboxed run reported no devices and CoreAudio OSStatus 560947818, while the same release binary succeeded with direct device access.
- Verification: Compared sandboxed and direct-device executions of the same binary.
- Follow-up: Keep automated logic tests sandbox-compatible and run clearly identified hardware smoke tests with CoreAudio access.

### 2026-06-13 - Stage-Two Decoder Spike Completed

- Type: completed
- Status: completed
- Change: Approved and locked Symphonia 0.6.0 with AAC, AIFF, FLAC, ID3v1, ID3v2, MP4/M4A, MP3, NEON, PCM, and WAV features. Added a bounded streaming decoder, normalized `MediaInfo`, accurate seek/reset handling, interleaved `f32` PCM chunks, a media-inspection CLI, original synthetic fixtures, and decoder documentation.
- Reason: Validate required local formats and establish a stable decoding boundary before connecting file I/O to the real-time engine.
- Verification: Eleven tests, formatting, clippy with warnings denied, and the release build pass. WAV, AIFF, FLAC, MP3, and AAC-LC/M4A decode to finite stereo 44.1 kHz `f32`; all five seek and continue decoding. Tagged MP3/M4A title and artist metadata pass. Corrupt media and invalid seeks return errors without panic. A release M4A inspection requested 1.500 seconds, reached 1.486 seconds, and returned a 1,024-frame PCM chunk.
- Follow-up: Build the one-deck transport using only already approved dependencies. Add gapless fixtures and representative real-library compatibility testing later. HE-AAC and HE-AACv2 remain unsupported.

### 2026-06-13 - Stage-Three One-Deck Transport Completed

- Type: completed
- Status: completed
- Change: Added a reusable Rust library boundary and a one-deck transport with decoder-worker isolation, bounded PCM and control queues, callback-safe buffer recycling, play, pause, gain, stop, seek, track replacement, EOF state, generation-based stale-data rejection, and health reporting. Added a playback CLI and transport test guide.
- Reason: Validate the complete local-file-to-CoreAudio path before introducing a second deck, mixer DSP, persistence, or the Tauri interface.
- Verification: Sixteen tests, formatting, clippy with warnings denied, and the release build pass. Silent CoreAudio smoke tests on the Apple M3 completed WAV, AIFF, FLAC, MP3, and AAC-LC/M4A playback with zero underflow callbacks, recycling failures, stream errors, or worker errors. WAV/AIFF/FLAC/MP3 rendered 132,300 frames each; M4A rendered 134,144. An M4A seek to 1.5 seconds completed as generation 2 with 68,608 rendered frames, 16 stale blocks discarded, and zero underflows or errors.
- Follow-up: Build the two-deck render graph with existing dependencies. Sample-rate conversion requires an explicit design and approval before any new dependency is added.

### 2026-06-13 - Stage-Four Two-Deck Engine Completed

- Type: completed
- Status: completed
- Change: Added a shared-callback two-deck engine with independent decoder pipelines, transport and channel-gain controls, equal-power crossfading, master gain, clipping protection and counters, per-deck snapshots, a two-deck CLI, deterministic mixer tests, and hardware-test documentation.
- Reason: Establish the central two-deck render graph and master clock before persistence, UI work, synchronization, or DSP expansion.
- Verification: Twenty tests, formatting, clippy with warnings denied, and the release build pass. A silent CoreAudio test simultaneously rendered WAV on deck A and AAC-LC/M4A on deck B for 263 shared callbacks. Deck A rendered 132,300 frames, deck B rendered 134,144, and both reached EOF with zero underflows, clipping, recycling failures, stream errors, or worker errors.
- Follow-up: Owner review of ADR-002 recommending Rubato 3.0.0 for worker-side fixed sample-rate conversion.

### 2026-06-13 - Sample-Rate Conversion Proposal

- Type: decision
- Status: proposed
- Change: Created `docs/architecture/ADR-002-sample-rate-conversion.md`, recommending Rubato 3.0.0 for fixed source-rate to engine-rate conversion on decoder workers.
- Reason: Mixed-rate music libraries are normal, and the current exact-rate restriction blocks dependable playback. Rubato is pure Rust, MIT-licensed, compatible with Rust 1.85, supports Apple-silicon acceleration, and provides preallocated processing APIs.
- Verification: Reviewed current primary Rubato documentation and repository information, plus libsamplerate and its Rust binding as alternatives. No dependency was installed.
- Follow-up: Owner approval is required before adding Rubato.

### 2026-06-13 - Stage-Five Sample-Rate Conversion Completed

- Type: completed
- Status: completed
- Change: Approved and locked Rubato 3.0.0. Added worker-side fixed sample-rate conversion with matching-rate bypass, FFT conversion, delay trimming, exact EOF flushing, and seek/track-replacement reset. Added 48 kHz and 96 kHz fixtures and conversion documentation.
- Reason: Remove the exact-rate restriction so normal mixed-rate music libraries can play through one engine/output rate.
- Verification: Twenty-four tests, formatting, clippy with warnings denied, and the release build pass. Automated tests verify 48→44.1 kHz and 96→48 kHz frame counts. A silent two-deck CoreAudio run converted 48 kHz and 96 kHz sources to 44.1 kHz; each emitted exactly 132,300 frames over 261 callbacks with zero underflows, clipping, recycling failures, stream errors, or worker errors. A resampled seek run also completed with zero underflows or errors.
- Follow-up: Owner review of ADR-003 for `rusqlite` and schema version 1. Reuse resampler worker allocations before long-duration performance acceptance.

### 2026-06-13 - Local Persistence Proposal

- Type: schema
- Status: proposed
- Change: Created `docs/architecture/ADR-003-local-persistence-schema.md`, recommending `rusqlite` 0.40.1 with bundled SQLite, a single-owner persistence worker, transactional migrations, and schema version 1 for settings, roots, tracks, analysis, corrections, cues, loops, and queue state.
- Reason: The next product capability requires durable local settings, library indexing, generated analysis references, and user-authored DJ metadata without cloud services.
- Verification: Reviewed current upstream `rusqlite` usage and bundled-SQLite guidance. No database dependency, table, or migration was added.
- Follow-up: Owner approval is required before adding `rusqlite` or creating schema version 1.

### 2026-06-13 - Stage-Six Local Persistence Completed

- Type: schema
- Status: completed
- Change: The owner approved ADR-003. Added and locked `rusqlite` 0.40.1 with bundled SQLite, embedded schema version 1, transactional migrations, newer-schema rejection, database pragmas, typed persistence operations, and a single-owner worker thread. Added `docs/testing/local-persistence.md` and set Rust 1.96 as the minimum supported toolchain.
- Reason: Persist application settings, library metadata, generated-analysis references, user corrections, cues, loops, and queue state locally without allowing database work onto the real-time audio path.
- Verification: Twenty-nine tests pass across the library and CLI. Persistence tests cover fresh creation, migration rollback, reopen durability, track insert/unchanged/modified/missing/restored states, constraints, cascades, queue order, and preserving user-authored corrections during re-analysis. Formatting, Clippy with warnings denied, and the locked release build pass on Rust 1.96.
- Follow-up: Present an application-scaffold ADR with exact Tauri, React, TypeScript, and Vite versions for owner approval. Wire the database to Tauri's application-data directory only after that approval.

### 2026-06-14 - Stage-Seven Application Scaffold Completed

- Type: completed
- Status: completed
- Change: The owner approved the application scaffold. Added a thin Tauri 2 macOS crate around the existing Rust engine, a React 19.2.7 and TypeScript 6.0.3 frontend built with Vite 8.0.16, locked npm and Cargo dependencies, a minimal capability policy, a read-only `engine_status` command, application-data SQLite startup, light/dark scaffold styles, and a temporary deterministic icon. Documented the architecture in ADR-004.
- Reason: Establish an installable desktop boundary and frontend-to-Rust bridge without moving or weakening the tested audio and persistence modules.
- Verification: Frontend dependency audit reports zero vulnerabilities; the production frontend build, 29 engine tests, engine and Tauri Clippy with warnings denied, engine release build, Tauri tests, and an integrated Tauri debug build pass. A direct macOS launch remained running without startup errors and created `djapp.sqlite` under `~/Library/Application Support/com.djapp.desktop` with schema version 1. The Tauri lockfile pins `alloc-stdlib` 0.2.2 to avoid an upstream Brotli allocator-version conflict.
- Follow-up: Obtain approval for recursive folder scanning and its Tauri command contract before adding filesystem capabilities or new production dependencies.

### 2026-06-14 - Stage-Eight Local Library Scanning Completed

- Type: completed
- Status: completed
- Change: The owner approved recursive folder selection and scanning. Added and locked Tauri dialog plugin 2.7.1 with open-only permission, a standard-library recursive scanner, supported-format filtering, Symphonia metadata extraction, SQLite root/track queries and reconciliation, blocking-worker Tauri commands, and an interim library table. Created ADR-005 and the scanner test guide.
- Reason: Let the DJ select local folder trees and reopen an indexed offline library without exposing general filesystem access to the frontend or touching source music.
- Verification: Thirty-two Rust tests pass, including nested scanning, corrupt-file fallback, missing detection, and restoration. Frontend and Tauri tests/builds pass; engine and Tauri Clippy with warnings denied, the engine release build, and npm production audit pass with zero reported vulnerabilities. No schema migration was added.
- Follow-up: Owner review of ADR-006 for the first functional deck-A UI playback slice.

### 2026-06-14 - Stage-Nine Deck A UI Playback Completed

- Type: completed
- Status: completed
- Change: The owner approved ADR-006. Added a dedicated deck-A service thread, indexed-track validation, Tauri load/play/pause/seek/stop/snapshot commands, functional React controls, library **Load A** actions, accurate output-rate position conversion, visible errors, and health counters. No dependency or schema change was required.
- Reason: Deliver the first complete UI-to-Rust-to-CoreAudio playback path while preserving ownership and real-time callback rules.
- Verification: Thirty-two engine/CLI tests and two Tauri service tests pass, with the hardware test ignored by default. Frontend build, engine and Tauri Clippy with warnings denied, and the integrated Tauri build pass. A silent Apple M3 CoreAudio test completed load, play, pause, seek, and stop over 61 callbacks with zero underflows, recycling failures, stream errors, or worker errors; 16 stale blocks were correctly discarded on generation changes.
- Follow-up: Owner review of ADR-007. Use one shared two-deck callback rather than adding a second standalone output stream.

### 2026-06-14 - Stage-Ten Shared Two-Deck UI Service Completed

- Type: completed
- Status: completed
- Change: The owner approved ADR-007. Adapted `MixerEngine` for independently unloaded decks, replaced the standalone Deck A owner with one shared mixer-service thread, added Deck B transport commands, returned combined 2 Hz snapshots, and connected React controls for both decks, channel gains, equal-power crossfader, and master gain. No dependency or schema change was required.
- Reason: Both decks must share one hardware stream and master clock while allowing either deck to load first and keeping all database and UI work outside the real-time callback.
- Verification: Thirty-two engine/CLI tests and the Tauri unloaded-service test pass; the hardware test remains ignored by default. Formatting, frontend production build, engine and Tauri Clippy with warnings denied, engine release build, and integrated Tauri debug app bundling pass. The Apple M3 mixed-rate CoreAudio test loaded 48 kHz and 96 kHz WAV fixtures, completed 91 shared callbacks, and reported zero clipping, underflows, recycling failures, stream errors, or worker errors. Deck B correctly discarded 16 stale blocks after seek.
- Follow-up: Owner review of an audio-device discovery, output selection, and stream-recovery milestone using the existing CPAL dependency.

### 2026-06-14 - Stage-Eleven Audio Device Selection And Recovery Completed

- Type: completed
- Status: completed
- Change: The owner approved ADR-008. Added CoreAudio output discovery using persistent CPAL device UIDs, a master-output selector, SQLite preference key `audio.output_device_id`, three-second hot-plug list refresh, controlled shared-engine restart with deck clock/play-state restoration, default-output fallback, callback-error recovery, and visible Bluetooth latency guidance. No dependency or schema change was required.
- Reason: Version 1 must support macOS-exposed built-in and external master outputs without crashes or competing streams when devices change or disconnect.
- Verification: Thirty-two engine/CLI tests and the Tauri unloaded-service test pass; the direct hardware test is ignored by default. Formatting, engine and Tauri Clippy with warnings denied, engine release build, frontend production build, and integrated Tauri debug app bundling pass. On the Apple M3, the mixed-rate test reselected the active CoreAudio output while both decks played, restored both absolute clocks and play states, then completed 61 callbacks with zero clipping, underflows, recycling failures, stream errors, or worker errors. Deck B correctly discarded 16 stale blocks after its later seek.
- Follow-up: Owner review of a separate headphone cue-routing and capability-detection architecture milestone.

### 2026-06-14 - Headphone Cue Routing Proposal

- Type: decision
- Status: proposed
- Change: Created ADR-009 recommending capability-gated stereo cue on one four-channel or greater CoreAudio device, with master on channels 1–2 and cue on channels 3–4. Separate-device clocking, automatic aggregate-device creation, configurable channel pairs, and mono split output remain deferred.
- Reason: One multichannel stream preserves a single hardware clock and reliable DJ timing, while two unrelated devices can drift and the current two-channel built-in output cannot provide private stereo cue.
- Verification: Direct CoreAudio enumeration on the target Apple M3 found a two-channel 44.1 kHz MacBook Pro Speakers output and a one-channel 48 kHz Teams virtual output. Current hardware therefore supports stereo master only and cannot validate full cue routing.
- Follow-up: Owner approval is required before changing the mixer callback, Tauri contract, persisted routing settings, or interim mixer UI.

### 2026-06-14 - Connected Headphone Capability Recheck

- Type: decision
- Status: proposed
- Change: Rechecked CoreAudio after wired headphones were connected. External Headphones and MacBook Pro Speakers now appear as separate stereo 44.1 kHz outputs, while Teams Audio remains mono at 48 kHz. Updated ADR-009 to include a possible dual-stream synchronization spike.
- Reason: The connected hardware changes what can be tested, but two separately clocked streams can drift or have different latency even when their nominal sample rates match.
- Verification: Direct device enumeration identified External Headphones as the macOS default output and retained MacBook Pro Speakers as a separate selectable output.
- Follow-up: Owner may approve the four-channel production design, the experimental dual-stream spike, or both. Neither has been implemented yet.

### 2026-06-14 - Stage-Twelve Headphone Cue Routing Implemented

- Type: completed
- Status: completed
- Change: The owner approved ADR-009. Added maximum-channel capability detection, explicit four-channel stream selection, master channels 1–2, pre-crossfader cue channels 3–4, Cue A/B, equal-power cue/master blend, independent cue gain, lock-free commands, persisted cue settings, restart restoration, and disabled master-only UI guidance. No dependency or schema change was required.
- Reason: Provide correct private stereo cue when one CoreAudio device supplies a shared clock and at least four channels, while never leaking cue into public master output on unsupported hardware.
- Verification: Thirty-six engine/CLI tests and the Tauri unloaded-service test pass by default. Deterministic tests cover capability thresholds, channel mapping, cue isolation, blend, gain, unused-channel silence, and master-only regression behavior. With wired headphones connected, the Apple M3 hardware test classified both External Headphones and MacBook Pro Speakers as master-only, rejected Cue A activation, and completed 61 callbacks with zero clipping, underflows, recycling failures, stream errors, or worker errors; 16 stale Deck B blocks were correctly discarded after seek.
- Follow-up: A four-channel interface or user-created aggregate device is required for physical channels 3–4 acceptance. Separate-device cue routing remains unapproved.

### 2026-06-14 - Dual-Output Synchronization Spike Completed

- Type: completed
- Status: completed
- Change: Added an isolated silent dual-output measurement module and CLI mode for separately exposed master and cue candidates. The spike opens two matching-rate CoreAudio streams, establishes a warm-up frame baseline, samples relative progress once per second, and reports drift and stream health without changing production routing.
- Reason: Determine whether MacBook Pro Speakers and External Headphones accumulate independent-clock drift before considering a production two-stream cue architecture.
- Verification: Thirty-nine engine/CLI tests, formatting, Clippy with warnings denied, and the release build pass. A 15-second validation had zero errors and one-quantum jitter. The uninterrupted 1,800-second Apple M3 run completed 155,853 speaker callbacks and 155,851 headphone callbacks with zero stream errors, zero final sampled drift, and maximum observed drift of 512 frames, about 11.6 ms at 44.1 kHz. No accumulating drift trend appeared.
- Follow-up: The spike does not measure fixed acoustic/output latency and did not test disconnect recovery or loaded shared buffering. Production dual-device routing requires a separate approved ADR and broader tests.

### 2026-06-14 - Production Dual-Device Cue Routing Proposal

- Type: decision
- Status: proposed
- Change: Created ADR-010 proposing an optional matching-rate, non-Bluetooth dual-device cue mode. The master callback remains the engine clock and fans the cue mix into a bounded lock-free queue for a minimal second callback. The proposal includes manual cue delay, queue-health telemetry, fail-closed cue recovery, persistence, UI contracts, and loaded hardware acceptance criteria.
- Reason: The 30-minute speaker/headphone spike showed no accumulating frame drift, making a carefully bounded production implementation credible on the target Mac, while fixed latency, loaded buffering, and device loss still require explicit controls and tests.
- Verification: Reconciled the proposal with ADR-008 recovery, ADR-009 cue privacy and single-device preference, the measured 512-frame scheduling jitter, current approved dependencies, and the existing SQLite settings model. No production code, dependency, schema, or established UI change was made.
- Follow-up: Owner approval is required before implementing ADR-010.

### 2026-06-14 - Stage-Thirteen Dual-Device Cue Routing Implemented

- Type: completed
- Status: completed
- Change: The owner approved ADR-010. Added matching-rate separate master/cue streams, master-clock stereo cue fanout through a bounded lock-free frame queue, manual 0-250 ms cue delay, pair validation, queue telemetry, persisted routing preferences, Tauri commands, interim UI controls, and fail-closed cue recovery that preserves master playback. No dependency or schema migration was added.
- Reason: The target Mac exposes built-in speakers and wired headphones as separate stereo devices, and the earlier 30-minute synchronization spike showed no accumulating frame drift.
- Verification: Forty engine/CLI tests and the default Tauri test pass; two CoreAudio tests remain ignored by default. Formatting, Rust Clippy with warnings denied, Tauri Clippy, the frontend production build, and integrated debug app bundling pass. A direct loaded two-deck Apple M3 run completed 261 master and 263 cue callbacks with queue depth bounded between 512 and 2,048 frames and zero underflows, overflows, or stream errors.
- Follow-up: Complete the 30-minute loaded run, audible cue-delay calibration, and physical headphone/master disconnect recovery checklist in `docs/testing/dual-device-cue-routing.md`.

### 2026-06-14 - Dual-Device Loaded Stability Accepted

- Type: completed
- Status: completed
- Change: Extended the ignored CoreAudio test into a duration-controlled loaded soak harness. It repeatedly seeks mixed-rate fixtures to keep both decoder/resampler pipelines active, tracks master and cue rendered frames, samples queue depth, and fails immediately on stream errors, cue underflow, cue overflow, or lost routing.
- Reason: The previous three-second test did not satisfy ADR-010's loaded 30-minute stability requirement.
- Verification: The 1,800-second Apple M3 run completed 155,066 master callbacks and 155,068 cue callbacks. It rendered 79,393,792 master frames and 79,394,816 cue frames; queue depth stayed between 512 and 2,048 frames, maximum relative deviation was one 512-frame callback quantum, and stream errors, cue underflows, and cue overflows all remained zero.
- Follow-up: Complete the interactive audible cue controls/delay check and physical cue/master disconnect recovery checklist. Loaded dual-device stability is accepted.

### 2026-06-14 - Professional Two-Deck UI And Automatic Cue

- Type: completed
- Status: completed
- Change: Rebuilt the interim interface around two track headers, waveform strips, original CSS-rendered platters, channel controls, a compact center mixer, a horizontal crossfader, audio-routing disclosure, and a dense lower library/AutoMix workspace inspired by the owner-provided reference without copying its branding or assets. Moving the crossfader left now cues Deck B, moving it right cues Deck A, and the center dead zone clears automatic cue because both decks are on master.
- Reason: The owner approved a professional two-deck visual direction and requested that the deck outside the crossfader mix be selected automatically for headphone cue.
- Verification: Forty engine/CLI tests and two default Tauri tests pass; the automatic cue thresholds have a deterministic service test. The TypeScript and Vite production build passes. Manual cue controls remain available until the next crossfader movement.
- Follow-up: Launch the redesigned app for visual review and complete audible cue/delay and physical device-loss acceptance.

### 2026-06-14 - Headphone Cue Signal Diagnostic

- Type: bug
- Status: completed
- Change: Restored visible Cue Level and Cue/Master blend controls removed during the UI redesign, and added peak-amplitude telemetry from samples consumed by the cue-device callback. Updated the hardware test to use low nonzero gains and automatic Deck B cue instead of testing only silence.
- Reason: External Headphones were detected but the owner could not hear the cued track, while the previous stability test had deliberately muted every signal and could not distinguish a live silent stream from real cue audio.
- Verification: A five-second direct CoreAudio run measured a nonzero cue peak of `0.00062491273` with 441 master callbacks, 443 cue callbacks, and zero stream errors, underflows, or overflows. Rust tests and the frontend production build pass.
- Follow-up: In the app, set Cue Level above zero and Cue/Master toward Cue. If the live cue-signal indicator is active but headphones remain silent, verify the macOS volume for External Headphones.

### 2026-06-15 - ADR-010 Hardware Accepted

- Type: completed
- Status: completed
- Change: The owner confirmed that separate-device headphone cue is audible and working correctly on the target Apple M3 with MacBook Pro Speakers as master and External Headphones as cue. ADR-010 is marked fully accepted.
- Reason: Manual audible confirmation completes the hardware evidence provided by the loaded 30-minute soak, nonzero cue-signal test, queue telemetry, and fail-closed recovery design.
- Verification: Owner confirmation on the target setup; automated and hardware results remain documented in `docs/testing/dual-device-cue-routing.md`.
- Follow-up: Proceed to the tempo, pitch, sync, and jog architecture milestone.

### 2026-06-15 - Tempo, Pitch, Sync, And Jog Proposal

- Type: decision
- Status: proposed
- Change: Created ADR-011 proposing a worker-side `TempoProcessor`, an isolated `signalsmith-stretch` 0.1.3 Apple-silicon spike, manual tempo/key-lock/pitch controls, bounded pitch bend, paused seek and playing nudge jog behavior, and capability-gated master/follower sync based on future BPM and beat-grid analysis.
- Reason: Version one requires professional deck speed and synchronization controls, while Rubato only handles fixed sample-rate conversion and the Signalsmith Rust wrapper still needs measured quality, latency, CPU, native-build, and sustained-playback validation.
- Verification: Reconciled the proposal with ADR-001, ADR-002, the current decoder/worker/callback ownership model, and current primary Signalsmith/Rubber Band licensing and API documentation. No dependency was installed and no production tempo code was added.
- Follow-up: Owner approval is required before adding `signalsmith-stretch` 0.1.3 for the isolated spike.

### 2026-06-15 - Signalsmith Tempo And Pitch Spike Completed

- Type: decision
- Status: completed
- Change: The owner approved ADR-011 and its isolated evaluation. Added `signalsmith-stretch` 0.1.3 as an optional feature-gated dependency and a standalone synthetic benchmark binary. The production engine, mixer, Tauri commands, and UI do not enable or call it.
- Reason: Validate native Apple-silicon build compatibility, latency, CPU headroom, tempo ratios, pitch accuracy, reset/flush behavior, and sustained two-deck processing before production integration.
- Verification: The crate built without patches using AppleClang C++14 and Bindgen. Unit tests, formatting, Clippy with warnings denied, and normal all-target checks pass. Both presets passed five tempo ratios and five pitch shifts; worst pitch error was 9.37 cents. Reset/flush stress remained finite. The default-preset release soak processed two concurrent 1,800-second stereo decks in 15.011 seconds with zero simulated buffered underflows; 23 isolated scheduling spikes were absorbed by its 120 ms processor buffer. The spike arm64 executable is 677,336 bytes and links only system `libc++` and `libSystem`.
- Follow-up: Complete manual listening acceptance, then request owner approval for production integration behind the ADR-011 `TempoProcessor` boundary. Beat sync remains deferred until BPM and beat-grid analysis exists.

### 2026-06-15 - Stage-Fourteen Production Tempo And Pitch Completed

- Type: completed
- Status: completed
- Change: The owner approved production Signalsmith integration. Added a project-owned per-deck `TempoProcessor` after fixed sample-rate conversion, production `signalsmith-stretch` 0.1.3 dependency, `-16%` to `+16%` tempo, default-on key lock, independent `-12` to `+12` semitone pitch, latency-aware EOF/reset handling, source-position tracking, device-recovery restoration, Tauri commands, snapshots, and functional deck controls. Sync remains disabled pending BPM and beat-grid analysis.
- Reason: Deliver dependable manual speed and pitch control without placing native processing or allocation in the CoreAudio callbacks or pretending beat synchronization is available before analysis exists.
- Verification: Forty-five engine/CLI/spike tests and two default Tauri tests pass; formatting, engine and Tauri Clippy with warnings denied, frontend production build, and release builds pass. A ten-second optimized Apple M3 run stretched both mixed-rate decks at different tempos, with key lock disabled and `+3` semitones on Deck B, while routing master to speakers and cue to headphones. It completed 882 master and 885 cue callbacks with zero stream errors, cue underflows, or cue overflows and a nonzero cue signal. An unoptimized run produced one cue underflow, so real-time performance acceptance remains release-only.
- Follow-up: Complete representative music listening acceptance. Then obtain approval for pitch-bend and jog implementation; BPM/beat-grid analysis is required before Sync.

### 2026-06-15 - Neutral Playback Repetition Regression Fixed

- Type: bug
- Status: superseded by Continuous Tempo Processing Timeline
- Change: Neutral `0%` tempo and `0`-semitone playback now bypasses Signalsmith and passes decoded/resampled PCM through unchanged with zero processor latency. Switching between bypass and stretched modes resets processor state, and latency telemetry updates when the mode changes.
- Reason: Routing every song through spectral time stretching even when no tempo or pitch effect was requested introduced unnecessary latency and could produce repeated-window artifacts or debug-build starvation during normal full-song playback.
- Verification: New tests prove sample-for-sample neutral passthrough, zero neutral latency, no neutral EOF flush output, and safe bypass/stretch transitions. The optimized direct CoreAudio mixed-rate two-deck test passed with zero deck underflows, stream errors, recycling failures, or worker errors.
- Follow-up: Manually replay the affected song at neutral settings. Non-neutral music quality listening remains part of Stage Fourteen acceptance.

### 2026-06-15 - Live Tempo Change Loop Regression Fixed

- Type: bug
- Status: superseded by Live Tempo Changes No Longer Seek and Continuous Tempo Processing Timeline
- Change: Tempo, key-lock, and pitch changes now create a new deck generation at the currently audible source position. The render queue rejects old blocks, and one atomic worker command applies settings, seeks the decoder, resets Signalsmith, clears pending output, and resumes the previous play state. No-op setting changes no longer restart transport.
- Reason: Live tempo changes previously reset Signalsmith while decoded and processed blocks from the old timeline remained queued. Combining those old blocks with the restarted processor could replay a short section and corrupt progression.
- Verification: The full automated suite and Clippy pass. An optimized Apple M3 hardware regression started both decks at neutral settings, changed them live to `-8%` and `+8%`, changed key lock and pitch, verified forward source progression, and completed ten seconds of repeated seek/reset activity with 974 master callbacks, 976 cue callbacks, nonzero headphone signal, and zero stream errors, cue underflows, or cue overflows.
- Follow-up: Manually retry the affected full song with live tempo changes. Listening-quality acceptance remains open.

### 2026-06-15 - Fader Pre-Roll And UI Race Fixed

- Type: bug
- Status: completed
- Status: superseded in part
- Change: Tempo and pitch sliders retain local state while dragged, ignore snapshot polling during the gesture, and submit the range element's exact release value. The initial input-pre-roll implementation was removed after real-song testing showed it duplicated post-change audio.
- Reason: UI snapshot polling could overwrite a slider during a drag, and pointer release could submit the previous React state value. The attempted pre-roll incorrectly supplied future samples as history and then processed the same samples again.
- Verification: Nonrepeating chirp tests continue to reject repeated streaming output windows. UI and audio regression suites remain active.
- Follow-up: Restart the desktop app and repeat the affected tempo/pitch gesture on the original song. Representative music listening acceptance remains open.

### 2026-06-15 - Tauri Dev Audio Optimization

- Type: bug
- Status: completed
- Change: The Tauri development profile now compiles the project audio engine and `signalsmith-stretch` at optimization level 3 while leaving the Tauri shell and frontend in their normal development profiles.
- Reason: `npm run tauri dev` previously ran the native time stretcher unoptimized. The isolated spike and hardware testing already showed that unoptimized Signalsmith can miss audio deadlines even when the release implementation is healthy.
- Verification: The Tauri development-profile Apple M3 hardware regression completed live `-8%` and `+8%` tempo changes plus key-lock and pitch changes over 973 master and 975 cue callbacks. It reported nonzero headphone signal and zero master/cue stream errors, cue underflows, or cue overflows.
- Follow-up: Retest the original song using a freshly rebuilt `npm run tauri dev` process.

### 2026-06-15 - Duplicate Fader Pre-Roll Removed

- Type: bug
- Status: completed
- Change: Removed the post-fader pre-roll buffer that passed the first future audio window to Signalsmith as seek history and then passed those same samples to streaming process. Live changes resume through the standard streaming process path.

### 2026-06-15 - Live Tempo Changes No Longer Seek

- Change: Tempo, key-lock, and pitch changes update Signalsmith in place on the decoder worker. They no longer increment the deck generation, clear queued audio, or seek the decoder.
- Reason: Compressed MP3/AAC seeks may resolve to an earlier packet. Seeking on every fader movement could replay the same source section and sound like a loop. Generation changes remain reserved for actual transport seeks and track replacement.
- Tradeoff: The control takes effect after the small bounded decoded queue already in flight, rather than immediately restarting at the audible position.
- Verification: The worker regression proves live changes remain in the current generation; the single-output and dual-output CoreAudio regressions both exercise live tempo changes and require forward source progression.
- Reason: The duplicate feed can audibly replay the first section after a tempo or pitch change on real music even though stationary sine-wave fixtures appear normal.
- Verification: Nonrepeating chirp and full regression tests are used to verify forward streaming, and the actual Tauri development executable is rebuilt before manual retesting.
- Follow-up: Retest the original song after fully stopping the previous app process.

### 2026-06-15 - Neutral-To-Stretch History Corrected

- Type: bug
- Status: superseded by Continuous Tempo Processing Timeline
- Change: Neutral bypass now retains one Signalsmith input-latency window of already-consumed PCM. Entering non-neutral tempo or pitch uses that past audio as seek history while processing every current and future sample exactly once. Actual transport resets clear the history.
- Reason: Starting the stretcher mid-song without preceding context can destabilize its first analysis windows. The earlier rejected pre-roll used future samples and duplicated them; this implementation uses only past samples and cannot replay them into the output timeline.
- Verification: A new chirp transition regression enters stretch after one second of neutral playback, produces non-silent output, and rejects repeated output windows.
- Follow-up: Repeat the original real-song BPM-change listening test.

### 2026-06-15 - Continuous Tempo Processing Timeline

- Type: bug
- Status: superseded; continuous neutral processing worsened real-song playback
- Change: Supersedes neutral bypass and transition-history approaches. Signalsmith now processes the deck continuously from track start, including neutral settings. Live BPM and pitch changes update the existing processor without reset, seek, pre-roll, or a bypass-to-stretch mode switch.
- Reason: The owner continued to hear a repeated section specifically when leaving neutral tempo. Keeping one processing timeline removes the last transition that could reintroduce already-heard spectral context.
- Tradeoff: Neutral playback carries the approved 120 ms processor latency and native processing cost. Tauri dev keeps the engine and Signalsmith optimized.
- Verification: Neutral duration accounting and a non-periodic chirp regression cover a live neutral-to-`+8%` change and reject repeated output windows.
- Follow-up: Confirm behavior on the affected real track.

### 2026-06-15 - Neutral Playback Restored, Tempo Issue Open

- Type: bug
- Status: unresolved
- Change: Restored direct PCM bypass at neutral tempo and pitch after the continuous Signalsmith experiment caused looping during ordinary playback. Non-neutral tempo remains available for diagnosis but is not accepted as reliable. The deck now labels the value as playback `RATE`, not BPM.
- Reason: Core playback reliability takes priority. Track BPM analysis is not implemented, but manual playback-rate processing does not require BPM metadata; analysis will later support BPM display, beat grids, Sync, and AutoMix.
- Verification: Neutral playback is sample-for-sample in automated tests, reports zero stretch latency, and does not flush processor output.
- Follow-up: Reproduce non-neutral processing with representative real audio before changing its production path again.

### 2026-06-15 - Unreliable Rate Adjustment Disabled

- Type: bug
- Status: completed safety mitigation; underlying tempo defect unresolved
- Change: Disabled the deck rate slider, added a visible `RATE LOCKED` state, made the Rust mixer reject every nonzero tempo command, and made device recovery restore `0%` instead of an unsafe rate.
- Reason: Real-song testing repeatedly confirms looping whenever non-neutral rate processing is engaged. Normal playback must remain dependable while the processor integration is reworked from representative audio evidence.
- Verification: Engine and Tauri tests require nonzero rate requests to fail and snapshots to remain at `0%`.
- Follow-up: Build a redistributable representative-music fixture or diagnostic capture before re-enabling rate adjustment.

### 2026-06-15 - Diagnostic Corpus And Varispeed Rate Fallback

- Type: bug
- Status: completed
- Change: Added an original 20-second music-like WAV fixture plus MP3/M4A encodings, a queued offline rate diagnostic with repeated-window detection, and a stateful project-owned linear varispeed processor. Re-enabled the rate slider as `VINYL RATE`; key lock and independent pitch remain disabled.
- Reason: Signalsmith passed controlled WAV/MP3/M4A diagnostics but repeatedly looped on the owner's music. Varispeed changes pitch with speed but avoids spectral analysis and its repeated-grain failure mode.
- Verification: Direct and 16-block queued diagnostics at `+8%` and `-8%` report no repeated windows for WAV, MP3, and M4A. Varispeed unit tests verify output duration and zero processor latency. The Apple M3 CoreAudio regression changed both playing decks to `-8%` and `+8%`, completed 60 callbacks, and reported zero underflows, clipping, stream errors, recycle failures, or worker errors.
- Follow-up: Complete target-hardware listening acceptance before considering the rate path final. Revisit key lock/pitch separately with representative licensed test material.

### 2026-06-15 - Engine-Rate Buffer Repetition Fixed

- Type: bug
- Status: completed
- Change: `EngineRateDecoder` now clears a recycled PCM buffer before copying the next resampler output into it. The full-track tempo diagnostic is capped at the 20-second rate-transition window, and a regression compares fresh-buffer decoding with production-style buffer reuse.
- Reason: For tracks whose native sample rate differs from the 48 kHz engine rate, the resampler appended each new converted block after samples retained from the previous block. This replayed earlier audio and made rate changes appear to loop on MP3 and affected WAV files; the defect was in sample-rate conversion, not BPM analysis or varispeed interpolation.
- Verification: The affected private 44.1 kHz MP3 reproduced an exact repeated window before the fix and reports `repeat_detected=false` afterward. Its 20-second diagnostic decoded-frame count fell from 1,845,280 accumulated frames to the expected 974,880. The reusable-buffer regression produces sample-for-sample output equal to fresh-buffer decoding.
- Follow-up: None; the owner confirmed the application works after rebuilding.

### 2026-06-15 - Track Analysis Pipeline Approved

- Type: decision
- Status: approved; stages one through three completed
- Change: Created `docs/architecture/ADR-012-track-analysis-pipeline.md`. It proposes a single bounded background worker, project-owned waveform/BPM/beat-grid/downbeat/key algorithms, versioned atomic cache files, confidence-based feature gating, and the existing SQLite schema. The only proposed new dependencies are `rustfft` and `ebur128`.
- Reason: Trustworthy cached analysis is required before BPM display, real waveforms, Sync, gain normalization, and analysis-driven AutoMix can be enabled without risking audio reliability or misleading the DJ.
- Verification: The owner approved ADR-012. Added pinned `rustfft` 6.4.1 and `ebur128` 0.1.10 dependencies, project-owned analysis/result types, versioned waveform and beat-grid cache codecs, and deterministic click/key fixtures. Fifty-one engine tests, six CLI tests, two Tauri tests, engine/Tauri Clippy with warnings denied, and the frontend production build pass. Cache tests cover round trips, truncation, version mismatch, invalid confidence, and non-monotonic beats.
- Follow-up: Completed by `Track Analysis Stage Two Completed` below.

### 2026-06-15 - Track Analysis Stage Two Completed

- Type: completed
- Status: completed
- Change: Added a bounded single-worker `AnalysisService`, injectable processor boundary, exact-identity deduplication, changed-identity replacement, cooperative cancellation, retained snapshots, queue limits, joined shutdown, result validation, and `pending`/`running`/`complete`/`failed` persistence transitions through the existing SQLite worker. Added analysis-record lookup without changing schema version 1.
- Reason: The waveform, loudness, rhythm, and key algorithms need a controlled non-real-time execution boundary that cannot block audio callbacks or create competing database ownership.
- Verification: Fifty-six engine tests, six CLI tests, two default Tauri tests, engine and Tauri Clippy with warnings denied, and the frontend production build pass. Service tests cover persisted running/completion, duplicate rejection, changed-file replacement, active cancellation, bounded queue rejection, queued cancellation, and clean shutdown.
- Follow-up: Stage three is completed below. Runtime `cancelled` still persists as `failed` with `analysis cancelled` because schema version 1 has no cancelled status; changing that requires migration approval.

### 2026-06-15 - Track Analysis Stage Three Completed

- Type: completed
- Status: completed
- Change: Added single-pass `WaveformLoudnessProcessor` analysis using the existing decoder and `ebur128`; a project-owned per-channel min/max/RMS waveform pyramid; source identity checks before and after decoding; deterministic cache naming; atomic temporary-file writes; and persistence of integrated LUFS, true peak dBTP, and waveform paths. Waveform format version 1 now uses little-endian `f32` triples and 256-frame base buckets with four-to-one coarser levels.
- Reason: Real waveform data and standards-based gain-normalization measurements are the first useful cached analysis products and establish the decode/cache path required by later BPM, beat-grid, and key stages.
- Verification: Sixty-four engine tests, six CLI tests, two default Tauri tests, engine/Tauri Clippy with warnings denied, and the frontend production build pass. Tests cover weighted RMS/extrema, stereo ordering, malformed PCM, cancellation, stale identity, atomic cache publication, all supported fixture formats, and end-to-end SQLite persistence.
- Follow-up: Stage four is completed below. Tauri lifecycle and React waveform display remain deferred to the approved UI/API integration stage.

### 2026-06-15 - Track Analysis Stage Four Completed

- Type: completed
- Status: completed
- Change: Added chunk-independent mono conversion to 22,050 Hz, streaming Hann-window RustFFT spectral-flux extraction, adaptive onset normalization, strict 60-200 BPM autocorrelation candidates, harmonic half/double-tempo scoring, sub-hop and long-span precision refinement, confidence output, BPM persistence, and the `bpm_benchmark` diagnostic.
- Reason: Reliable BPM metadata is required before beat-grid tracking and later Sync, but it must reject silence and stationary tones, expose uncertainty, and remain outside the audio callback.
- Verification: The deterministic 60/90/120/128/150/180 BPM benchmark has a worst error of 0.009645%. The music-like fixture measures 119.811985 BPM; accented and syncopated fixtures resolve to 120 BPM; silence and steady tones return no BPM. Seventy-three engine tests, six CLI tests, two default Tauri tests, both Clippy runs with warnings denied, and the frontend build pass.
- Follow-up: Stage five is completed below. Do not enable Sync until the later corpus and playback-under-analysis acceptance milestone.

### 2026-06-15 - Track Analysis Stage Five Completed

- Type: completed
- Status: completed
- Change: Added dynamic-programming onset tracking, weighted beat-grid fitting, grid confidence, conservative four-beat downbeat inference, rational source-frame conversion, atomic versioned beat-grid caches, SQLite path persistence, and the `beat_grid_benchmark` diagnostic.
- Reason: Beat positions on the original track timeline are required before Sync or beat-aware transitions can be evaluated, while uncertain meter evidence must not create false downbeats.
- Verification: Deterministic 60-180 BPM, accented, and syncopated fixtures have median absolute timing errors from 9.524 ms to 11.882 ms, below the approved 20 ms target. Only the accented fixture receives downbeats. All 76 engine tests, eight CLI tests, two default Tauri tests, both strict Clippy runs, and the frontend production build pass; two hardware-only CoreAudio tests remain ignored.
- Follow-up: Stage six is completed below. Sync remains disabled pending labeled-corpus and playback-under-analysis acceptance.

### 2026-06-15 - Track Analysis Stage Six Completed

- Type: completed
- Status: completed
- Change: Added streaming chroma and tuning analysis, major/minor template scoring, confidence and rejection gates, canonical key persistence, validated correction writes, an effective-analysis read model with manual BPM/key/grid-offset precedence, and the `key_benchmark` diagnostic.
- Reason: Key metadata and correction precedence are required before analysis can be exposed reliably to the library UI or used as a compatibility signal without re-analysis overwriting user choices.
- Verification: All 24 synthetic major/minor keys classify exactly; A minor remains correct from -35 through +35 cents; silence, broadband noise, and pitched percussion return no key. Correction tests prove overrides remove generated confidence from the effective value while preserving the original generated record. All 81 engine tests, eight CLI tests, two default Tauri tests, both strict Clippy runs, the key benchmark, frontend production build, and diff checks pass; two hardware-only CoreAudio tests remain ignored.
- Follow-up: Stage seven is completed below. Sync remains disabled.

### 2026-06-15 - Track Analysis Stage Seven Completed

- Type: completed
- Status: completed
- Change: Started the bounded analysis worker in Tauri, added analyze/cancel/job/correction/artifact commands, identity-based stale detection, intermediate progress reporting, enriched library results, and existing-layout Analyze/Cancel/Correct UI controls with confidence indicators.
- Reason: Generated analysis must be observable, cancellable, safely cache-validated, and correction-aware in the desktop application before playback stress tests or Sync acceptance can be meaningful.
- Verification: All 81 engine tests, eight CLI tests, three default Tauri tests, both strict Clippy runs, the frontend production build, and diff checks pass; two hardware-only CoreAudio tests remain ignored. Tauri coverage includes current-versus-changed source identity, while engine coverage includes progress, cancellation, caches, algorithms, and correction precedence.
- Follow-up: Stage eight is completed below. Do not enable Sync yet.

### 2026-06-15 - Track Analysis Stage Eight Completed

- Type: completed
- Status: completed with benchmark limitation
- Change: Added cache-reopen-without-source coverage, active production cancellation checks, and a muted Apple M3 CoreAudio harness that runs complete analysis while two mixed-rate decks play. Re-ran deterministic BPM, beat-grid, key, supported-format, and application verification.
- Reason: Analysis must demonstrate cache durability, cancellation safety, and no playback-health regression on target hardware before its metadata can be considered for live feature gating.
- Verification: Apple M3 callbacks advanced from 43 to 219 during full analysis with zero stream errors, deck underflows, recycle failures, or worker errors. BPM worst error remains 0.009645%, beat-grid median error remains 9.524-11.882 ms, and all 24 synthetic keys classify exactly. Cache reopen succeeds after source removal and active cancellation publishes no cache paths. All 83 engine tests, eight CLI tests, three default Tauri tests, both strict Clippy runs, the frontend production build, benchmarks, hardware acceptance run, and diff checks pass; three direct-CoreAudio tests remain ignored by default.
- Follow-up: Completed by Track Analysis Stage Nine.

### 2026-06-15 - Track Analysis Stage Nine Completed

- Type: decision
- Status: completed
- Change: Completed ADR-012's feature-gating milestone. Analysis remains enabled for library display, cache reuse, manual corrections, queue UX, and diagnostics. Master/follower Sync, beat-aware AutoMix timing, compatibility-based AutoMix reordering, downbeat-dependent transitions, and automatic generated-value correction remain disabled.
- Reason: Available deterministic and Apple M3 stability evidence is sufficient for metadata display but not sufficient for DJ-grade live features. Owner feedback shows generated BPM is inaccurate on at least some real music compared with other software, and no labeled real-music benchmark or representative full-library soak has been accepted.
- Verification: Updated ADR-012 and added `docs/testing/analysis-feature-gating.md` to record the accepted evidence, missing evidence, active gates, and next acceptance work.
- Follow-up: Build a private benchmark manifest with representative tracks and expected BPM values, then tune BPM ambiguity handling and confidence thresholds before reconsidering Sync or analysis-driven AutoMix.

### 2026-06-15 - BPM Accuracy Milestone Started

- Type: tooling
- Status: in progress
- Change: Extended `bpm_benchmark` with private TSV manifest and segment-diagnostic modes and documented the workflow in `docs/testing/bpm-accuracy-milestone.md`. Added ignore rules for `private-benchmarks/`, `*.local.tsv`, and `*.local.csv`. After the first owner run showed 0/5 accepted exact-BPM real tracks, added candidate diagnostics, capped BPM confidence for ambiguous top candidates, added 20-second segment-consensus candidates with half/double variants, added beat-support reranking, weighted low/low-mid spectral flux, and added octave correction for near-tied half/double candidates. The next pass removed tempo-range forcing and added neutral candidate grid diagnostics for beat strength, offbeat contrast, stability, section consistency, and octave ambiguity. Later passes added dependency-free low, low-mid, mid, and high rhythm envelopes with cross-band candidate generation, `bands` diagnostics, temporal tempo-state scoring over overlapping 12-second windows with `state` diagnostics, scratch-built comb-filter/tempogram-style `comb` diagnostics, dynamic-programming beat-sequence `seq` diagnostics, explicit tempo-octave resolver `oct` diagnostics, and bounded precision-refinement `fit` diagnostics. Bumped `ANALYSIS_VERSION` from 1 to 11 so stale overconfident, pre-consensus, pre-reranking, pre-grid-diagnostic, pre-multi-band, pre-tempo-state, pre-comb-filter, pre-beat-sequence, pre-octave-resolver, and pre-precision-refinement results are invalidated.
- Reason: Real-song BPM accuracy cannot be tuned from synthetic fixtures alone. The owner needs repeatable comparison against reference BPM values from other DJ software without committing private music or paths. Candidate diagnostics distinguish bad ranking from missing onset evidence, and confidence calibration must not mark wrong real-song estimates as trustworthy.
- Verification: Private manifest acceptance improved from 0/5 to 4/5 exact BPMs after precision refinement. `ffawty - Stay Home` now passes at 151.298 BPM for expected 151.0, `Lil Tecca - Down With Me` passes at 115.140 BPM for expected 115.0, `U Fancy` passes at 148.194 BPM for expected 148.0, and `LUCKI - MORE THAN EVER` passes at 151.991 BPM for expected 152.0. `Lil Gnar - Welcome 2 Da Game` remains the hardest failure: the correct-neighborhood 75.418 BPM candidate is present and within the 1% gate for expected 76.0, but it ranks behind 94.233 BPM.
- Follow-up: Re-run Analyze Missing after rebuilding so analysis version 11 refreshes cached BPM and confidence. The next BPM work should target hard wrong-family ranking for `Lil Gnar`-style tracks and expand the private benchmark toward the ADR's 25-track representative set.

### 2026-06-16 - BPM Accuracy Dependency Decision Approved

- Type: decision
- Status: approved; in-house spike stage five completed
- Change: Added `docs/architecture/ADR-013-bpm-accuracy-and-rhythm-dependency-decision.md` comparing continued project-owned deep beat tracking, aubio, Essentia, and madmom for BPM/beat accuracy. Owner approval selected the in-house spike first. Stage one added temporal tempo-state scoring without new dependencies, stage two added scratch-built comb-filter/tempogram-style candidate scoring, stage three added dynamic-programming beat-sequence support with `seq` diagnostics, stage four added an explicit tempo-octave resolver with `oct` diagnostics, and stage five added bounded precision refinement with `fit` diagnostics plus a narrow near-tie evidence breaker. The aubio comparison path has a non-production `aubio_compare` diagnostic binary that shells out to an installed `aubiotrack` command and estimates BPM from beat timestamps. Installed aubio `0.4.9` through Homebrew for local diagnostics only.
- Reason: The private BPM benchmark remains 1/5 after several dependency-free tuning passes, so the project needs an explicit decision before adding copyleft/native rhythm dependencies or continuing deeper in-house algorithm work.
- Verification: Checked current upstream project documentation for aubio, `aubio-rs`, Essentia, and madmom. Focused rhythm and benchmark tests pass. The private five-track manifest now reaches 4/5 for the project-owned analyzer after `fit`, but generated BPM still must not drive Sync or AutoMix because the ADR gate is 90% on a larger representative set. Default `aubiotrack` accepted 0/5 exact BPMs on the private manifest, so aubio is not a drop-in accuracy fix.
- Follow-up: aubio remains GPL and is not approved for shipping or Tauri integration. Further aubio work, if any, should stay diagnostic and compare alternate `aubiotrack` onset methods or thresholds. The next project-owned BPM step should target hard wrong-family ranking for `Lil Gnar`-style tracks, while manual BPM correction UX remains the practical safety valve.

## Update Template

Use this structure for future meaningful entries:

```markdown
### YYYY-MM-DD - Short Title

- Type: decision | completed | bug | schema | API | technical debt | future work
- Status: proposed | approved | completed | superseded | unresolved
- Change: What changed.
- Reason: Why it changed.
- Verification: Tests or checks performed.
- Follow-up: Remaining work or `None`.
```
