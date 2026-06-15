# ADR-011: Tempo, Pitch, Beat Sync, And Jog Control

- Status: Accepted for architecture and isolated Signalsmith spike; production integration still requires owner approval
- Date: 2026-06-15
- Scope: Per-deck tempo and pitch processing, master/follower beat synchronization, pitch bend, and pointer-driven jog behavior

## Context

The current engine decodes each track, converts its fixed source sample rate to the CoreAudio engine rate, and feeds bounded PCM blocks to the shared mixer callback. Playback speed is fixed. The UI contains visual placeholders for Sync, tempo, and platters, but they do not yet control audio.

Version one requires independent tempo adjustment, pitch bend, jog interaction, master-deck selection, beat synchronization, and optional key lock. ADR-002's Rubato stage only performs fixed source-rate conversion and must not be repurposed for DJ tempo control.

Beat synchronization also depends on trustworthy BPM and beat-grid data. That analysis pipeline is not implemented yet, so this milestone must separate usable manual tempo/jog controls from sync features that require analyzed timing data.

## Options

### Signalsmith Stretch

The upstream C++11 library provides independent time stretching and pitch shifting, reset/seek support, explicit input/output latency, and a split-computation option. It is MIT licensed and documents its best time-stretch quality between approximately `0.75x` and `1.5x`.

The Rust wrapper is currently `signalsmith-stretch` `0.1.3`. It is MIT licensed and documented, but it is a small native wrapper with limited adoption and uses C++ compilation plus Bindgen during its build.

- Strengths: permissive license, appropriate DJ tempo range, independent pitch and tempo, macOS/AppleClang is an upstream-tested configuration, small runtime dependency surface.
- Risks: wrapper maturity, native build tooling, C++ object lifecycle, algorithm latency, CPU spikes without split computation, and unknown artifact quality on the target music corpus.
- Decision: preferred candidate for an isolated spike, not yet approved as a production dependency.

### Rubber Band

Rubber Band is mature, actively developed, cross-platform, and purpose-built for independent tempo and pitch control.

- Strengths: mature API and broad real-world use.
- Risks: GPL-2.0-or-later unless a commercial license is purchased, larger native integration surface, and Apple App Store distribution restrictions under the open-source license.
- Decision: fallback only if Signalsmith fails and the owner separately approves the licensing and distribution cost.

### Project-Owned Variable-Rate Resampling

A small interpolating resampler can change speed and pitch together for vinyl-style playback, pitch bend, and basic jog nudging.

- Strengths: no dependency, low latency, deterministic behavior, useful as a fallback and test oracle.
- Risks: no key lock, audible quality loss at larger ratios, duplicated work once a production stretcher is adopted, and insufficient quality for the full requirement.
- Decision: use only for bounded transient jog/pitch-bend behavior or as a degraded fallback, not as the main tempo engine.

## Recommendation

Adopt a two-stage plan:

1. Approve an isolated `signalsmith-stretch` `0.1.3` Apple-silicon spike.
2. Approve production integration only after the spike passes quality, latency, CPU, seek/reset, and sustained two-deck tests.

Keep all third-party code behind a project-owned `TempoProcessor` interface. If the wrapper proves unsuitable, replace it with a minimal project-owned FFI boundary around a pinned upstream Signalsmith release without changing deck or Tauri contracts.

Do not add Rubber Band, a database migration, or BPM-analysis dependencies in this milestone.

## Processing Architecture

The per-deck pipeline becomes:

```text
MediaDecoder
  -> fixed EngineRateDecoder (Rubato, existing)
  -> tempo/pitch processing worker
  -> bounded processed-block queue
  -> DeckRender in the CoreAudio callback
```

Rules:

- One processing worker and stretcher state per loaded deck.
- The CoreAudio callback only consumes ready processed blocks and applies existing mixer DSP.
- Stretcher construction, destruction, reset, seek pre-roll, allocation, and native calls stay off the callback.
- Input and output buffers are preallocated and recycled through bounded queues.
- Tempo and pitch commands use a bounded control queue and are applied at block boundaries.
- Control changes use short ramps to avoid discontinuities.
- Seek, track replacement, loop wrap, and stop reset the processor generation so stale stretched audio cannot play.
- Position reporting distinguishes source position, processing latency, and audible output position.

## Control Model

Each deck owns:

- `tempo_percent`: default `0`; initial UI range `-16%` to `+16%`.
- `tempo_ratio`: derived as `1 + tempo_percent / 100`.
- `key_lock`: default enabled once the production stretcher is accepted.
- `pitch_semitones`: default `0`; initial independent range `-12` to `+12` semitones.
- `pitch_bend`: momentary additive rate offset, initially up to `4%`.
- `sync_enabled`: whether the deck follows the selected master.
- `is_sync_master`: exactly one loaded deck may be master.
- `jog_mode`: `nudge` while playing and `seek` while paused.

For key lock:

- Tempo changes alter duration without changing musical pitch.
- Independent pitch shift changes pitch without changing duration.
- With key lock disabled, tempo may use linked vinyl-style pitch behavior.

## Beat Synchronization

Sync is capability-gated by valid BPM and beat-grid data.

- A deck without analyzed BPM/grid may use manual tempo and jog controls, but Sync is disabled with an explanation.
- Pressing Sync on the follower sets its target tempo ratio from `master_bpm / follower_bpm`.
- Tempo matching is limited to the approved range; impossible ratios are rejected rather than silently clamped into a wrong match.
- Phase alignment uses the analyzed beat grid and the audible timeline after processor latency compensation.
- Small phase error is corrected with bounded temporary pitch bend, not a destructive seek.
- Large phase error while paused may use a quantized seek; large error during playback requires explicit user action initially.
- Disabling Sync leaves the current tempo in place so playback does not jump.
- Manual tempo movement disables follower Sync; momentary pitch bend and jog nudge do not.

Master selection is explicit in the UI. AutoMix may choose a master later, but that is outside this ADR.

## Jog And Pitch-Bend Behavior

### Playing Deck

- Horizontal platter drag applies a temporary signed rate bend.
- Releasing the platter ramps the bend back to zero over approximately 50 ms.
- Pitch-bend buttons apply fixed `-4%` and `+4%` while held.
- Nudge must not permanently change the tempo fader or disable Sync.

### Paused Deck

- Platter drag changes the pending source position with coarse/fine scaling based on drag distance.
- Audio processing is reset and pre-rolled after the gesture.
- The first implementation updates position during drag and commits one seek on release; continuous scratch audio is deferred.

Full vinyl scratching, reverse playback, inertia simulation, and motor start/stop effects require a later separately approved design.

## Service And UI Contract

Proposed Tauri commands:

- `deck_a_set_tempo(percent)` / `deck_b_set_tempo(percent)`
- `deck_a_set_key_lock(enabled)` / `deck_b_set_key_lock(enabled)`
- `deck_a_set_pitch(semitones)` / `deck_b_set_pitch(semitones)`
- `deck_a_set_pitch_bend(percent)` / `deck_b_set_pitch_bend(percent)`
- `deck_a_jog(delta_frames, commit)` / `deck_b_jog(delta_frames, commit)`
- `mixer_set_sync_master(deck)`
- `deck_a_set_sync(enabled)` / `deck_b_set_sync(enabled)`

Snapshots add tempo, pitch, key-lock state, effective rate, processor latency, BPM/grid capability, sync role/status, phase error, and processor health counters.

The approved visual layout gains functional tempo faders, Sync/master controls, pitch-bend buttons, key-lock controls, and pointer/trackpad platter gestures. This is an extension of the approved layout, not a redesign.

## Persistence

Global preferences such as default tempo range and default key-lock state may use the existing `settings` table without a migration.

Per-session deck tempo, bend, and sync state are not persisted initially. BPM and beat-grid persistence remains governed by the existing schema and a later analysis implementation.

## Testing And Acceptance

### Signalsmith Spike

- Build and run on arm64 Apple M3 in debug and release profiles.
- Report native build requirements and final bundle impact.
- Measure processor input/output latency for default and cheaper presets.
- Measure CPU for one and two stereo decks at `0.75x`, `0.8x`, `1.0x`, `1.25x`, and `1.5x`.
- Run seek/reset, rapid automation, EOF flush, and track-replacement stress tests.
- Complete a 30-minute two-deck release soak with zero callback underflows or native failures.
- Perform listening tests on percussion, vocals, bass-heavy music, and sustained harmonic material.

### Production Behavior

- Tempo accuracy within `0.1%` on synthetic click fixtures.
- Pitch accuracy within 20 cents for independent pitch tests.
- No stale audio after seek, loop, or track replacement.
- Pitch-bend and jog release return smoothly without clicks.
- Sync tempo reaches the mathematically correct ratio.
- With valid beat grids, settled phase error remains within 10 ms under steady playback.
- Manual tempo disables follower Sync; jog nudge does not.
- Existing master/cue routing remains stable under two stretched decks.
- UI pointer gestures remain usable without a controller or keyboard.

## Incremental Plan

1. Run and document the isolated Signalsmith wrapper spike; do not connect it to Tauri.
2. If accepted, add the project-owned `TempoProcessor` interface and worker-side processed queue.
3. Implement manual tempo, key lock, independent pitch, snapshots, and UI controls.
4. Implement momentary pitch bend and playing/paused jog gestures.
5. Implement BPM and beat-grid analysis in its separately approved milestone.
6. Enable master/follower tempo sync, then phase sync when valid beat grids exist.
7. Run two-deck, dual-output, latency, and manual DJ workflow acceptance.

## Tradeoffs

This order delivers useful manual deck control before beat analysis is ready while avoiding fake Sync behavior. It introduces native C++ build complexity and additional per-deck latency, but keeps that risk off the real-time callback and behind a replaceable interface. Deferring continuous scratch audio prevents a large specialized transport project from delaying reliable tempo and beat matching.

## Approval Request

Approval authorizes:

- The architecture, control semantics, worker boundaries, and incremental plan in this ADR.
- An isolated Apple-silicon spike adding `signalsmith-stretch` `0.1.3` only to evaluate it.
- Synthetic fixtures, benchmarks, listening-test documentation, and spike-only CLI/test code.

Approval does not yet authorize production integration of Signalsmith, Rubber Band, a database migration, BPM/beat-grid dependencies, final Sync enablement without analyzed grids, continuous scratch audio, reverse playback, or changes outside macOS.

## Approval

The owner approved this ADR and its isolated Signalsmith evaluation on 2026-06-15. The evaluation remains feature-gated and disconnected from the production deck, mixer, Tauri, and UI paths until its results receive separate approval.

## Spike Outcome

The approved Apple-silicon spike completed on 2026-06-15. `signalsmith-stretch` 0.1.3 built without patches, passed synthetic tempo and pitch checks, completed reset/flush stress, and processed two concurrent 30-minute stereo workloads with zero simulated buffered underflows. The default preset reports 120 ms total processor latency and is recommended over the 140 ms cheaper preset. Detailed measurements and remaining listening acceptance are recorded in `docs/testing/signalsmith-tempo-pitch-spike.md`.

The dependency remains optional and feature-gated. This result recommends production integration behind `TempoProcessor`, but does not authorize it.
