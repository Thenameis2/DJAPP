# Project Memory

## How To Use This File

Read this file before every task and update it after every meaningful change. Keep confirmed decisions, current state, completed work, bugs, schema changes, API contracts, and future work accurate. Mark replaced decisions as superseded instead of deleting them.

## Current State

- Status: Stage-thirteen dual-device cue loaded stability accepted; professional two-deck UI redesign and crossfader-follow cue behavior implemented; manual audio acceptance remains pending.
- Target: private, offline-first macOS DJ desktop application.
- Target hardware: Apple M3 Mac.
- Implementation: reusable Rust engine plus a Tauri 2, React, TypeScript, and Vite macOS application scaffold.
- Production architecture: approved.
- Database schema: version 1 created and tested.
- External APIs: none.
- Known bugs: none confirmed. Physical channels 3–4 cue output has not yet been tested because no four-channel device or aggregate device is currently available. External Headphones can receive nonzero cue samples; audible volume still requires manual confirmation.

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
- Internal Tauri commands include library scanning/query; output-device discovery and selection; Deck A and Deck B load/play/pause/seek/stop; `mixer_snapshot`; per-deck gain and cue selection; crossfader; master gain; cue gain; and cue/master blend. Deck transport commands return the combined mixer snapshot. No external API contracts exist.

## Known Issues And Risks

- Target machine runs macOS 26.4.1 on arm64 Apple M3 hardware.
- CPAL 0.18.1, `rtrb` 0.3.4, Symphonia 0.6.0, Rubato 3.0.0, and `rusqlite` 0.40.1 with bundled SQLite are approved and locked. Remaining production dependencies still require staged approval.
- The project minimum toolchain is Rust 1.96 because the approved bundled SQLite build does not compile on the previous Rust 1.85 toolchain.
- Exact effects for version 1 have not been selected.
- Performance and latency targets have not been benchmarked.
- Headphone cue without separate audio hardware may be limited by macOS device capabilities.
- CPAL's CoreAudio backend may report `unknown` interface type for some Bluetooth devices, so the UI displays wireless latency guidance beside all output selections.
- Public distribution would introduce signing, notarization, licensing, privacy, and support requirements.
- Sample-rate conversion allocates temporary output/adapter buffers on decoder workers; allocation reuse remains a performance optimization.
- The temporary application icon and scaffold layout are not approved final branding or production UI design.
- The theme toggle is session-only until a settings command is connected to SQLite.
- Scans skip symlinks. If traversal errors occur, missing-file reconciliation is skipped for that scan to prevent false missing records.
- One dedicated mixer-service thread owns both decks and the single CoreAudio stream; Tauri commands never hold or share the stream directly. UI snapshots poll at 2 Hz.

## Next Meaningful Tasks

1. Complete physical cue acceptance with a four-channel interface or user-created macOS aggregate device.
2. Complete ADR-010 manual acceptance: audible latency calibration and physical cue/master device-loss recovery.
3. Continue with the next core DJ control milestone after dual-device acceptance.

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
