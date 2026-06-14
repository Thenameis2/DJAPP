# ADR-001: Audio, Analysis, Time-Stretching, And Persistence Dependencies

- Status: Approved by owner on 2026-06-13
- Date: 2026-06-13
- Decision owners: Project owner and senior developer
- Scope: macOS Apple silicon, Tauri, React/TypeScript, Rust, and SQLite

## Context

The application needs two low-latency decks, local decoding, track analysis, independent tempo and pitch control, waveform generation, device routing, and local persistence. It must run offline on an Apple M3 Mac and remain suitable for possible public distribution.

The project architecture is approved, but production dependencies are not. This ADR compares candidates and recommends a small dependency set. Nothing in this document authorizes dependency installation.

The ecosystem has a meaningful licensing constraint: several mature music-information-retrieval libraries are GPL or AGPL. Those licenses may be acceptable for a fully GPL application, but they would constrain future distribution and are therefore not recommended for the initial architecture.

## Decision Drivers

1. Stable low-latency operation on `aarch64-apple-darwin`.
2. Explicit audio-device enumeration and routing.
3. No allocation, blocking, I/O, or locks in the real-time callback.
4. MP3, WAV, FLAC, AAC/M4A, and AIFF support.
5. Licenses compatible with private use and possible later public distribution.
6. Strong Rust integration and manageable Tauri packaging.
7. A small dependency and native-code surface.
8. Testable deterministic behavior outside the audio callback.

## Proposed Decision

Approve these dependencies in stages, not all at once:

| Purpose | Proposed choice | License | Confidence |
| --- | --- | --- | --- |
| Audio I/O and device routing | `cpal` | Apache-2.0 | High |
| Audio decoding and metadata | `symphonia` with selected features | MPL-2.0 | High |
| Real-time queues | `rtrb` | MIT OR Apache-2.0 | High |
| Local SQLite access | `rusqlite` with `bundled` | MIT; SQLite public domain | High |
| Loudness and true peak | `ebur128` | MIT | High |
| Spectral analysis foundation | `rustfft` | MIT OR Apache-2.0 | High |
| Time-stretch and pitch-shift | `signalsmith-stretch`, conditional on prototype | MIT | Medium-low for wrapper; high for upstream algorithm |

Use custom project code over decoded PCM for waveform generation, BPM estimation, beat-grid tracking, downbeat confidence, and musical-key estimation. This avoids GPL/AGPL dependencies, but those algorithms must pass a quality benchmark before version-one acceptance.

Do not initially use `rodio`, `sqlx`, `aubio`, Essentia, libKeyFinder, Rubber Band, or FFmpeg.

## Candidate Comparison

### Audio Playback And Device Routing

#### CPAL

- Apple silicon: Explicit `aarch64-apple-darwin` documentation and a CoreAudio backend. CPAL 0.18.1 documents macOS 14.2 as its current minimum for CoreAudio, so the target Mac's exact macOS version must be confirmed before pinning it.
- Maintenance and maturity: Active RustAudio project with frequent releases; version 0.18.1 was current when researched.
- License: Apache-2.0.
- Performance: Low-level callback API suitable for a custom mixer. Buffer size can be selected within device capabilities.
- Tauri compatibility: Native Rust library in the Tauri backend. The UI communicates through commands/events and never owns the stream.
- Tradeoff: CPAL provides I/O, not a DJ engine. Mixing, clocks, buffering, resampling, device-loss recovery, and routing remain application responsibilities.

Recommendation: Use CPAL as the only top-level audio I/O abstraction. Keep a thin project-owned adapter so CoreAudio-specific work can be introduced later without leaking CPAL types through the engine.

Source: [CPAL repository](https://github.com/RustAudio/cpal) and [CPAL documentation](https://docs.rs/cpal/latest/cpal/).

#### Rodio

- Apple silicon: Inherits support from CPAL.
- Maintenance and maturity: Active, mature high-level playback library.
- License: MIT OR Apache-2.0.
- Performance: Appropriate for conventional playback and sinks.
- Tauri compatibility: Straightforward Rust integration.
- Tradeoff: Its high-level playback model adds an abstraction that the application would largely bypass for two synchronized decks, per-sample mixer controls, cue routing, beat-accurate transport, and custom time-stretching.

Decision: Do not use. CPAL plus a project-owned engine is smaller and gives the required control.

Source: [Rodio repository](https://github.com/RustAudio/rodio).

#### coreaudio-rs Or Direct Apple APIs

- Apple silicon: Native platform API access.
- Maintenance and maturity: `coreaudio-rs` is established but lower-level and has no published GitHub releases; CPAL already uses CoreAudio-related crates on macOS.
- License: MIT OR Apache-2.0.
- Performance: Maximum platform control, with more unsafe and lifecycle complexity.
- Tauri compatibility: Compatible in the Rust backend.
- Tradeoff: Locks the engine to macOS implementation details early and increases device-management code.

Decision: Keep behind the audio-device adapter as a fallback only if CPAL cannot expose required channel maps, device identity, aggregate-device behavior, or hot-plug events.

Source: [coreaudio-rs repository](https://github.com/RustAudio/coreaudio-rs).

### Audio Decoding

#### Symphonia

- Apple silicon: Pure Rust with NEON optimization support; no external decoder installation.
- Maintenance and maturity: Active; version 0.6.0 was released in May 2026. MP3, FLAC, and PCM are rated excellent by the project; AAC-LC and AIFF/MP4 are rated great.
- License: MPL-2.0.
- Format coverage: MP3, WAV, FLAC, AAC-LC in MP4/M4A, AIFF, ALAC, and metadata via explicit feature flags.
- Performance: Designed for efficient demuxing and decoding with minimal dependencies.
- Tauri compatibility: Pure Rust backend dependency with simple app bundling.
- Tradeoff: HE-AAC and HE-AACv2 are not supported. Some unusual AAC files may fail. MPL notices and source-file obligations for modifications to MPL-covered files must be respected.

Recommendation: Use only required features rather than `all`: `aac`, `aiff`, `flac`, `isomp4`, `mp3`, `pcm`, and `wav`, plus required metadata features. Validate real-world M4A and AIFF fixtures before declaring format support complete.

Source: [Symphonia repository](https://github.com/pdeljanov/Symphonia) and [Symphonia documentation](https://docs.rs/symphonia/latest/symphonia/).

#### FFmpeg Bindings

- Apple silicon: Strong codec coverage when FFmpeg is built correctly.
- Maintenance and maturity: Very mature upstream; Rust wrappers and build setups add another maintenance layer.
- License: LGPL or GPL depending on build configuration and enabled components.
- Performance: Excellent, but broader than needed.
- Tauri compatibility: Requires native library bundling, architecture-aware builds, license auditing, and signing/notarization care.
- Tradeoff: Larger binary and packaging surface, more complex updates, and avoidable licensing decisions.

Decision: Do not use initially. Reconsider only if the supported-file test corpus exposes unacceptable Symphonia gaps.

Source: [FFmpeg legal information](https://ffmpeg.org/legal.html).

#### AVFoundation Through Rust Bindings

- Apple silicon: Native and well integrated with macOS codecs.
- Maintenance and maturity: Apple framework is mature; application-side Rust bindings and conversion code would be project-specific.
- License: Platform framework, subject to Apple SDK terms.
- Performance: Good and hardware/platform optimized.
- Tauri compatibility: Possible through Objective-C bindings, but increases platform-specific code.
- Tradeoff: Harder deterministic cross-library testing and a larger unsafe boundary.

Decision: Reserve as a fallback decoder adapter for unsupported AAC variants after evidence from the codec test suite.

### BPM, Beat Grid, And Downbeats

#### Custom Rust Analysis Using RustFFT

- Apple silicon: Pure Rust; RustFFT automatically supports NEON on AArch64.
- Maintenance and maturity: RustFFT is mature and active. The project-owned analysis algorithms will initially be immature.
- License: MIT OR Apache-2.0.
- Performance: Analysis runs offline on worker threads, so it can favor quality over real-time latency.
- Tauri compatibility: Pure Rust backend work with progress events to React.
- Proposed algorithm: mono analysis signal, resampling to a fixed analysis rate, spectral-flux onset envelope, adaptive peak picking, autocorrelation or tempogram tempo candidates, dynamic-programming beat tracking, half/double-tempo scoring, and confidence values. Downbeat detection should remain explicitly confidence-based.
- Tradeoff: More implementation and validation work than a mature MIR library. Accuracy is not assumed.

Recommendation: Prototype this path and compare it against a labeled fixture corpus. Store the algorithm version and confidence with every result so future re-analysis is controlled.

Source: [RustFFT documentation](https://docs.rs/rustfft/latest/rustfft/).

#### aubio

- Apple silicon: Builds on macOS, but is a C library requiring FFI and native packaging.
- Maintenance and maturity: Mature tempo and beat algorithms, but the latest formal release shown by the project is from 2019.
- License: GPL-3.0-or-later.
- Performance: Suitable for analysis and real-time use.
- Tauri compatibility: Technically compatible through bindings; increases native build and distribution obligations.
- Tradeoff: GPL would constrain distribution of the whole application.

Decision: Do not use unless the owner deliberately chooses a GPL-compatible distribution model.

Source: [aubio repository](https://github.com/aubio/aubio).

#### Essentia

- Apple silicon: Supports macOS; native C++ integration is required.
- Maintenance and maturity: Broad, mature MIR toolkit with strong descriptor coverage.
- License: AGPL-3.0, with separate licensing potentially available.
- Performance: Optimized but substantially larger than the required subset.
- Tauri compatibility: Possible, but native C++ packaging and AGPL obligations are significant.
- Tradeoff: Best ready-made feature breadth, largest architecture and license cost.

Decision: Do not use for the initial application.

Source: [Essentia repository](https://github.com/MTG/essentia).

#### SoundTouch BPM Detection

- Apple silicon: C++ library builds on macOS.
- Maintenance and maturity: Long-lived and current; its BPM method focuses on repeating low-frequency patterns.
- License: LGPL-2.1, with a commercial alternative.
- Performance: Lightweight.
- Tauri compatibility: Requires FFI and compliant dynamic-linking/distribution work.
- Tradeoff: BPM estimation is narrower than the required beat-grid/downbeat pipeline and can misread uneven or complex bass patterns.

Decision: Not selected as the analysis foundation.

Source: [SoundTouch README](https://www.surina.net/soundtouch/README.html).

### Musical-Key Detection

#### Custom Chroma Analysis Using RustFFT

- Apple silicon: Pure Rust with AArch64 NEON support.
- Maintenance and maturity: FFT foundation is mature; project key estimation must be validated.
- License: MIT OR Apache-2.0.
- Performance: Suitable for background analysis.
- Tauri compatibility: Pure Rust.
- Proposed algorithm: tuning estimation, harmonic/percussive-aware spectral weighting where practical, log-frequency chroma accumulation, silence/percussion rejection, major/minor template correlation, temporal voting, and confidence output.
- Tradeoff: Key estimates vary by genre and tuning. The UI must permit manual correction.

Recommendation: Prototype and benchmark against labeled tracks. Never store a key without algorithm version and confidence.

#### libKeyFinder

- Apple silicon: C++11 and portable to macOS.
- Maintenance and maturity: Focused, mature key detector now maintained by the Mixxx team.
- License: GPL-3.0-or-later.
- Performance: Appropriate for offline analysis.
- Tauri compatibility: Requires C++ FFI and GPL-compatible distribution.
- Tradeoff: Strong focused option, but licensing conflicts with preserving distribution flexibility.

Decision: Do not use unless the project adopts GPL distribution.

Source: [libKeyFinder repository](https://github.com/ibsh/libKeyFinder).

#### Essentia

Essentia provides mature tonal descriptors, but the AGPL and native C++ costs described above remain. It is not selected.

### Waveform Generation

#### Project-Owned PCM Peak Pyramid

- Apple silicon: Platform-neutral Rust.
- Maintenance and maturity: Simple deterministic algorithm owned by the project.
- License: Project license.
- Performance: One pass over decoded PCM. Generate fixed-size min/max/RMS buckets and progressively coarser levels for overview and zoom views.
- Tauri compatibility: Persist compact binary blobs or files and send only visible windows to React.
- Tradeoff: Requires a versioned cache format and careful handling of very long files.

Recommendation: Do not add a waveform-specific dependency. Reuse the Symphonia decode pass and analysis workers. Keep detailed data outside SQLite if benchmarks show BLOB growth or query contention; store its path, checksum, and format version in SQLite.

Alternatives such as rendering directly in React or storing every sample are rejected because they increase UI traffic and memory use.

### Loudness And Peak Analysis

#### ebur128

- Apple silicon: Pure Rust and documented for `aarch64-apple-darwin`.
- Maintenance and maturity: Focused implementation ported from libebur128; passes the EBU TECH 3341/3342 tests according to the project.
- License: MIT.
- Performance: Comparable to the C implementation and appropriate for background analysis.
- Tauri compatibility: Pure Rust.
- Tradeoff: Adds a dependency for a specialized standard, but avoids writing and validating loudness filters and gating logic.

Recommendation: Use for integrated loudness, loudness range when useful, and true peak. Compute simple sample peak during the same decode pass.

Source: [ebur128 repository](https://github.com/sdroege/ebur128) and [ebur128 documentation](https://docs.rs/ebur128/latest/ebur128/).

### Time-Stretching And Pitch-Shifting

#### Signalsmith Stretch With Rust Wrapper

- Apple silicon: Upstream is mostly tested with AppleClang and can use Apple Accelerate. The Rust wrapper compiles C++ through a build script.
- Maintenance and maturity: Upstream algorithm is credible and documented; the Rust wrapper is small, pre-1.0, and has limited adoption.
- License: MIT for upstream and wrapper.
- Performance: Intended for block processing; upstream documents latency and a cheaper preset. Best stated quality range for stretching is approximately 0.75x to 1.5x, appropriate for normal DJ tempo ranges.
- Tauri compatibility: Compatible in principle, but C++ compilation, universal/native architecture builds, panic/exception boundaries, and app signing must be tested.
- Tradeoff: Best permissive option found, but the wrapper is the weakest-maintained part of the proposed stack. It must not allocate inside the CPAL callback.

Recommendation: Conditional. Build a standalone Apple-silicon spike after approval. Feed and drain the stretcher on a dedicated processing worker using preallocated blocks and bounded `rtrb` queues. The CPAL callback only consumes ready output. Approve for production only after artifact, CPU, latency, seek/reset, and sustained-playback tests pass.

Source: [Signalsmith Stretch upstream](https://github.com/Signalsmith-Audio/signalsmith-stretch), [Rust wrapper](https://github.com/colinmarc/signalsmith-stretch-rs), and [wrapper documentation](https://docs.rs/signalsmith-stretch/latest/signalsmith_stretch/).

#### SoundTouch

- Apple silicon: Builds on macOS.
- Maintenance and maturity: Mature C++ library with tempo, pitch, rate, and BPM functionality.
- License: LGPL-2.1 or commercial.
- Performance: Designed for real-time processing with quality/CPU options.
- Tauri compatibility: Requires FFI and LGPL-compliant library replacement or relinking strategy when distributed.
- Tradeoff: More distribution and native packaging complexity than MIT-licensed Signalsmith; quality must still be evaluated for full mixes.

Decision: Keep as the fallback if Signalsmith fails quality or stability tests.

#### Rubber Band

- Apple silicon: Official CI includes macOS/iOS; mature real-time and offline engines.
- Maintenance and maturity: Mature, high-quality, widely used.
- License: GPL-2.0-or-later or commercial. The project explicitly warns that GPL builds cannot be distributed through Apple's app stores.
- Performance: High-quality R3 costs substantially more CPU; R2 is faster.
- Tauri compatibility: Native C/C++ integration is possible but packaging and licensing are substantial.
- Tradeoff: Technically strong, but not a no-cost permissive dependency.

Decision: Do not use without a deliberate GPL decision or commercial-license budget.

Source: [Rubber Band repository](https://github.com/breakfastquay/rubberband) and [licensing page](https://breakfastquay.com/rubberband/license.html).

#### Pure Resampling

Resampling changes tempo and pitch together and is useful for vinyl-style pitch control, scratch/jog effects, sample-rate conversion, and an early transport prototype. It does not satisfy key-lock or independent pitch shifting. It may be implemented first but is not the final version-one solution.

### SQLite Integration

#### rusqlite

- Apple silicon: Rust wrapper over SQLite; the bundled feature compiles a known SQLite version for the target.
- Maintenance and maturity: Mature and active; version 0.40.1 was current when researched.
- License: MIT; bundled SQLite is public domain.
- Performance: Direct synchronous API with low overhead. Database work must remain off the audio thread.
- Tauri compatibility: Straightforward backend integration and predictable application bundling.
- Tradeoff: The application owns its worker-thread model, migrations, and typed mapping.

Recommendation: Use `rusqlite` with `bundled`, one persistence service, WAL mode where appropriate, short transactions, explicit migrations, and no connection access from real-time code.

Source: [rusqlite repository](https://github.com/rusqlite/rusqlite) and [documentation](https://docs.rs/rusqlite/latest/rusqlite/).

#### SQLx

- Apple silicon: Supported.
- Maintenance and maturity: Mature multi-database async toolkit with compile-time query support.
- License: MIT OR Apache-2.0.
- Performance: Good, but an async runtime and pool are unnecessary for a local single-user application.
- Tauri compatibility: Compatible, with more runtime and feature complexity.
- Tradeoff: Larger dependency surface and build complexity for capabilities this application does not need.

Decision: Do not use initially.

Source: [SQLx repository](https://github.com/transact-rs/sqlx).

### Real-Time Communication

#### rtrb

- Apple silicon: Pure Rust and documented for `aarch64-apple-darwin`.
- Maintenance and maturity: Focused, active crate; version 0.3.4 was current when researched.
- License: MIT OR Apache-2.0.
- Performance: Preallocated, lock-free, wait-free SPSC reads and writes.
- Tauri compatibility: Pure Rust backend.
- Tradeoff: SPSC only. Each producer/consumer path needs its own queue, and overflow/underflow policy must be explicit.

Recommendation: Use bounded queues for commands, decoded blocks, stretched blocks, and meter snapshots. Queue element types must not allocate when moved or processed by the audio callback.

Source: [rtrb documentation](https://docs.rs/rtrb/latest/rtrb/) and [repository](https://github.com/mgeier/rtrb).

## Proposed Module Boundaries

```text
src-tauri/src/
  app/             Tauri commands, event translation, lifecycle
  audio/
    device/        CPAL adapter, capabilities, routing, hot-plug recovery
    engine/        render graph, master clock, deck mixing, meters
    deck/          transport, cue, loops, hot cues, rate and sync state
    dsp/           gain, EQ, filter, crossfader, limiter, effects
    stretch/       project interface plus Signalsmith implementation
    queue/         typed rtrb channels and overflow policy
  media/
    decode/        Symphonia adapter and normalized PCM blocks
    metadata/      tags and source-file identity
    library/       folder scan, reconciliation, duplicate policy
  analysis/
    pipeline/      jobs, cancellation, versions, progress
    rhythm/        BPM, beats, downbeats, confidence
    key/           tuning, chroma, key, confidence
    waveform/      multiresolution peak/RMS cache
    loudness/      ebur128 and sample peak
  persistence/
    connection/    rusqlite worker ownership
    migrations/    ordered, transactional schema changes
    repositories/  typed persistence interfaces
  automix/         queue scoring and transition planning
  domain/          dependency-free domain types and invariants
```

React receives serializable snapshots and commands. It never receives raw audio buffers, opens SQLite, or controls CPAL objects directly.

## Audio-Thread Safety Rules

1. Allocate all buffers, queues, DSP state, FFT plans, and stretcher state before playback.
2. Never allocate, free, log, format strings, open files, decode, analyze, access SQLite, call Tauri, or wait in the CPAL callback.
3. Never take a mutex, read-write lock, condition variable, or blocking channel in the callback.
4. Use fixed-capacity SPSC queues and atomics for bounded commands and snapshots.
5. Define overflow and underflow behavior: retain the last parameter value, emit silence or a short preallocated fade on starvation, and report counters outside the callback.
6. Keep destructors for large buffers and native stretcher objects off the callback thread.
7. Convert all deck audio to one internal planar `f32` format and engine sample rate before the final callback stage.
8. Treat device changes as a controlled engine restart with state restoration, not in-place mutation from the UI thread.
9. Measure worst-case callback duration, deadline misses, underruns, and queue depth in release builds.
10. Isolate all C++ FFI behind a Rust interface and prevent exceptions or panics from crossing the audio callback boundary.

## Testing Strategy

### Automated Unit And Property Tests

- Deterministic transport, seek, loop-wrap, cue, sync, crossfader, gain, EQ, and filter tests.
- No-NaN/no-infinity and bounded-output properties for DSP controls.
- Queue overflow, underflow, ordering, and stale-command behavior.
- Decoder fixtures for each promised container/codec combination, including corrupt and truncated files.
- Waveform pyramid invariants and stable cache serialization.
- Migration upgrade tests from every released schema version.

### Analysis Quality Corpus

- Small redistributable fixtures with known BPM, beat timestamps, key, loudness, and silence sections.
- Synthetic click tracks covering 60-200 BPM, swing, tempo changes, half-time, and silence.
- Musical fixtures covering electronic, hip-hop, R&B, pop, reggae, and tracks with weak percussion.
- Report median BPM error, half/double-tempo error rate, beat F-measure within a documented tolerance, and key accuracy including relative-major/minor and neighboring-key metrics.
- Results below approved thresholds block acceptance but do not crash or hide low confidence.

### Audio Engine Integration Tests

- Offline render tests compare output hashes or toleranced sample metrics.
- Two-deck sustained render with simultaneous analysis load.
- Seek, loop, hot-cue, tempo-change, and stretcher-reset stress tests.
- Device-loss simulation at the adapter boundary.

### Apple M3 Manual And Performance Tests

- Built-in speaker, wired headphone, Bluetooth output, and any available multi-output/aggregate device.
- Buffer-size sweep with CPU, callback deadline, underrun, and end-to-end latency measurements.
- At least a two-hour uninterrupted two-deck playback test in a release build.
- Signalsmith listening tests across representative tracks and tempo ratios before production approval.
- Tauri signed development build inspection for bundled native code and architecture.

## Incremental Implementation Plan

1. Approve only CPAL and `rtrb`; build a command-line audio-device and callback stability spike.
2. Approve Symphonia; decode each required format into normalized PCM and test seeking.
3. Build one deck, then two decks, with project-owned mixing and simple resampling only.
4. Approve `rusqlite`; define the schema in a separate owner-approved ADR and add migrations.
5. Add project-owned waveform generation and approve `ebur128` for loudness/true peak.
6. Approve RustFFT and prototype BPM/beat-grid/key analysis against the quality corpus.
7. Approve a time-stretch spike using Signalsmith; accept or reject it based on measured quality, CPU, latency, and wrapper risk.
8. Add sync, loops, hot cues, mixer DSP, and routing capability detection.
9. Add AutoMix only after beat grids and transition timing meet approved quality thresholds.
10. Integrate the React interface incrementally through stable Tauri command/event contracts.

Each stage has a separate approval gate for new production dependencies. A failed spike may use development-only dependencies or standalone code, but nothing moves into the application manifest without approval.

## Consequences And Risks

### Benefits

- Small, mostly pure-Rust and permissively licensed foundation.
- Direct control over real-time behavior and macOS devices.
- Codec coverage without bundling FFmpeg.
- Clear escape hatches behind project-owned adapters.
- Analysis results can evolve through explicit algorithm and cache versions.

### Costs

- BPM, beat-grid, downbeat, and key quality require substantial project work.
- Signalsmith's Rust wrapper is not mature enough to accept without a spike.
- CPAL does not solve multi-device clock drift or impossible headphone-routing configurations.
- MPL notice and modification obligations for Symphonia must be tracked.

### Revisit Triggers

Reopen this ADR if:

- Symphonia fails more than an approved fraction of the representative music corpus.
- CPAL cannot provide required macOS routing or stable device identity.
- Custom rhythm/key analysis cannot meet approved quality targets.
- Signalsmith fails sustained playback, latency, or listening tests.
- Public distribution changes the acceptable license model or provides a commercial-library budget.

## Approval

The owner approved this ADR on 2026-06-13. This approves the direction and staged evaluation order. It does not authorize all dependencies at once. The first requested implementation approval after this ADR is for CPAL and `rtrb` only, to build the audio-device and callback stability spike.

### Stage 1 Approval And Result

The owner approved CPAL and `rtrb` on 2026-06-13. The spike locked CPAL 0.18.1 and `rtrb` 0.3.4 and passed unit tests, clippy with warnings denied, a release build, CoreAudio device enumeration, and a three-second silent callback test on an Apple M3 Mac running macOS 26.4.1.

The observed default output was the two-channel MacBook Pro Speakers at 44.1 kHz with `f32` samples and a reported buffer range of 14 to 4096 frames. The run completed 260 callbacks and 266,240 interleaved samples, acknowledged two bounded commands, reported zero stream errors, and stopped cleanly. These are viability observations, not final latency or stability benchmarks.

### Stage 2 Approval And Result

The owner approved Symphonia on 2026-06-13 with `aac`, `aiff`, `flac`, `id3v1`, `id3v2`, `isomp4`, `mp3`, `opt-simd-neon`, `pcm`, and `wav` features. Symphonia 0.6.0 is locked.

The decoder spike passed WAV/PCM, AIFF/PCM, FLAC, MP3, and AAC-LC in M4A decoding using original synthetic fixtures. It also passed accurate seeking, normalized title and artist metadata, finite interleaved `f32` conversion, clean EOF, invalid-seek handling, and corrupt-file rejection. A release inspection of the M4A fixture requested 1.500 seconds, reported an actual seek point of 1.486 seconds, and returned a bounded 1,024-frame stereo chunk at 44.1 kHz.

This validates the selected integration but does not expand Symphonia's documented codec support. HE-AAC and HE-AACv2 remain unsupported, and representative user-library testing is still required before declaring format compatibility complete.

### Stage 3 One-Deck Transport Result

No new dependency was required. On 2026-06-13, the approved CPAL, `rtrb`, and Symphonia stack was connected into a one-deck transport with a decoder worker, bounded PCM and control queues, recycled buffers, generation-based seek/stop/track replacement, EOF handling, and health counters.

WAV, AIFF, FLAC, MP3, and AAC-LC/M4A each completed silent release-mode playback through CoreAudio on the Apple M3 target with zero underflows, recycling failures, stream errors, or worker errors. A seek-generation smoke test discarded stale blocks and completed with zero underflows or errors. The transport currently requires the media and output-device sample rates to match; adding sample-rate conversion remains a separately reviewed implementation decision.

### Stage 4 Two-Deck Engine Result

No new dependency was required. On 2026-06-13, two independent decoder/transport pipelines were combined under one CPAL callback and master clock. The engine includes independent deck controls and channel gains, an equal-power crossfader, master gain, clipping counters, per-deck health reporting, and deterministic mixer tests.

A silent Apple M3 CoreAudio test simultaneously rendered WAV on deck A and AAC-LC/M4A on deck B. Both reached EOF with zero underflows, clipped samples, recycling failures, stream errors, or worker errors. Sample-rate conversion remains the next approval gate and is proposed separately in ADR-002.

## Research Notes

Sources were checked on 2026-06-13. Version numbers are observations, not pins. Compatible versions must be selected and locked when each dependency is separately approved and installed.
