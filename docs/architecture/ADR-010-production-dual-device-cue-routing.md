# ADR-010: Production Dual-Device Cue Routing

- Status: Approved and implemented; manual hardware acceptance pending
- Date: 2026-06-14
- Scope: Separate stereo master and headphone-cue outputs on matching-rate macOS devices

## Context

ADR-009 implemented the preferred headphone-cue design: one CoreAudio stream routes master to channels 1-2 and cue to channels 3-4. That design has one hardware clock, but it requires a four-channel interface or aggregate device.

On the target Apple M3, wired headphones expose two separate stereo 44.1 kHz outputs:

- MacBook Pro Speakers
- External Headphones

The approved synchronization spike ran both silent streams for 1,800 seconds. It measured zero final drift, no accumulating trend, zero stream errors, and a maximum sampled difference of 512 frames, approximately 11.6 ms. This supports a production experiment on this device pair, but it does not measure fixed output latency, loaded mixer behavior, or disconnection recovery.

## Decision

Add an explicit `dual-device-cue` routing mode for separately exposed, matching-rate stereo outputs. Preserve these existing modes:

- `master-only`: one stereo output.
- `single-device-cue`: preferred mode using one four-channel-or-greater output.
- `dual-device-cue`: optional mode using separate stereo master and cue devices.

Dual-device mode is available only when:

- Master and cue are different output-device UIDs.
- Both devices expose stereo output.
- Their selected stream configurations use the same nominal sample rate.
- Neither route is known to be Bluetooth for the initial implementation.

Mismatched-rate devices, automatic aggregate-device creation, asynchronous sample-rate conversion, and Bluetooth dual-device routing remain deferred.

## Engine Design

The master-device callback remains the canonical engine clock.

For each master callback it will:

1. Render Deck A and Deck B once.
2. Produce the normal stereo master mix.
3. Produce the selected stereo cue/master-monitor mix from the same engine frames.
4. Write master directly to the master device.
5. Push interleaved cue samples into a preallocated bounded SPSC queue.

The cue-device callback only drains the cue queue and writes stereo output. It must not decode media, advance deck state, recalculate the mix, block, allocate, log, access SQLite, or acquire a mutex. On queue underflow it writes silence and increments a counter. On overflow the master callback preserves master playback, drops only the excess cue samples, and increments a counter.

Use the already approved `rtrb` dependency. Add no production dependency.

The queue will be sized from frames, not callback count. The initial target is:

- 2,048-frame prefill, approximately 46.4 ms at 44.1 kHz.
- 4,096-frame capacity, approximately 92.9 ms at 44.1 kHz.
- Low and high watermarks exposed in health telemetry.

Exact constants may be tuned by deterministic and hardware tests without changing this architecture.

## Clock And Latency Policy

Matching nominal sample rates are mandatory, but they do not prove identical clocks. Production health telemetry must include:

- Master and cue rendered-frame counts.
- Cue queue depth and observed minimum/maximum depth.
- Cue underflow and overflow counts.
- Master and cue stream-error counts.
- Relative frame progress from an established post-warm-up baseline.

The first implementation will not continuously resample or silently slip frames. If queue depth repeatedly reaches a boundary or relative progress develops a sustained trend, cue routing fails closed: master continues, cue is stopped, and the UI explains that the device pair is not stable enough for dual-device cue.

Separate outputs may have a fixed latency difference even when their clocks advance together. Add a persisted manual `cue_delay_ms` adjustment from 0 to 250 ms. It adds delay to cue through queue prefill; it cannot make cue earlier than master. The default is 0 ms. A later calibration tool may use CoreAudio timestamps or an audible test signal, but automatic acoustic calibration is outside this milestone.

## Ownership And Recovery

The existing mixer-service thread owns both streams and the cue queue.

- Cue-device loss: keep master playing, stop only cue, retain deck state, mark dual-device cue unavailable, and never redirect cue to master.
- Master-device loss: stop both streams and use the ADR-008 controlled master recovery. Resume in master-only mode unless the selected pair is available and the user explicitly re-enables dual-device cue.
- Cue callback error: stop cue and preserve master.
- Master callback error: invoke controlled master recovery and stop the dependent cue stream.
- Device refresh: continue the existing three-second discovery cycle.
- Reconnection: do not automatically send private cue audio to a newly discovered or default device.

## Persistence And API Contract

Use the existing `settings` table; no schema migration is required.

Proposed settings:

- `audio.routing_mode`
- `audio.output_device_id` for master, already present
- `audio.cue_output_device_id`
- `audio.cue_delay_ms`

Proposed Tauri commands:

- `audio_select_cue_output_device(device_id)`
- `audio_set_routing_mode(mode)`
- `audio_set_cue_delay_ms(delay_ms)`

The mixer snapshot adds selected cue-device identity, dual-device availability and active state, limitation text, queue health, stream errors, and latency adjustment. Existing cue A/B, cue gain, and cue/master blend commands remain unchanged.

## UI Behavior

Extend the interim audio controls with:

- A routing-mode selector.
- A cue-output selector.
- A cue-delay control.
- Clear reasons when a pair is unavailable, including same-device selection, mono output, sample-rate mismatch, Bluetooth, or unstable queue health.

The four-channel single-device route remains labeled as preferred. This is a functional extension of the interim UI, not final visual-design approval.

## Testing And Acceptance

Automated tests must cover:

- One render pass feeding master and cue from the same engine frames.
- Partial callback reads and queue wraparound.
- Prefill, underflow, overflow, and watermark accounting.
- Master continuity when cue underflows, errors, or disconnects.
- Rejection of the same device, mono devices, mismatched rates, and Bluetooth pairs.
- Settings restoration without automatically exposing cue audio.
- Existing master-only and four-channel routing regressions.

Apple M3 hardware acceptance must include:

- A loaded two-deck 30-minute run using MacBook Pro Speakers for master and External Headphones for cue.
- Zero master underflows and stream errors.
- Zero cue underflows after prefill and no cue overflows.
- Queue depth remaining bounded without a sustained trend.
- Relative progress remaining within one observed callback quantum of its baseline.
- Audible verification of Cue A, Cue B, cue/master blend, and cue gain.
- Cue disconnect/reconnect while master continues without interruption or cue leakage.
- Master disconnect and controlled recovery.
- Manual fixed-latency evaluation and documentation of the selected cue-delay value.

Failure of any stability or privacy criterion keeps dual-device mode experimental and unavailable by default.

## Tradeoffs

This design enables private stereo cue with the target Mac's current built-in routes and avoids duplicate deck rendering. It adds a second callback, buffering latency, more recovery states, and hardware-specific behavior. The queue can absorb callback scheduling jitter but cannot correct truly different device clocks. Restricting the first release to matching-rate, non-Bluetooth devices keeps the implementation testable and avoids adding asynchronous resampling to the real-time path.

## Incremental Implementation Plan

1. Add routing-mode and pair-validation types with deterministic tests.
2. Add the bounded cue fanout queue and callback tests without exposing it through Tauri.
3. Add dual-stream ownership, telemetry, and cue-loss recovery to the mixer service.
4. Add settings and Tauri commands using the existing persistence schema.
5. Extend the interim controls and failure guidance.
6. Run the loaded 30-minute, latency, and disconnect acceptance tests.
7. Mark the mode production-ready only if every acceptance criterion passes.

## Approval Request

Approval authorizes:

- Production implementation of the `dual-device-cue` mode described above.
- Master-clock cue fanout using the existing `rtrb` dependency.
- Separate cue-device selection, manual cue delay, health telemetry, and recovery behavior.
- New settings keys in the existing table and the listed Tauri/UI contract changes.
- Tests and target-hardware acceptance work.

Approval does not authorize new dependencies, a database migration, automatic aggregate-device creation, asynchronous resampling, Bluetooth dual-device routing, mono split output, final UI design, or support beyond macOS.

## Implementation Result

The owner approved ADR-010 on 2026-06-14.

- `MixerEngine` can open different matching-rate stereo master and cue devices.
- The master callback renders both decks once and pushes complete stereo cue frames through a bounded `rtrb` queue.
- The cue callback only drains queued frames, converts samples, writes silence on underflow, and updates atomics.
- A 2,048-frame base prefill prevents startup starvation on the target route. Manual delay extends both prefill and queue capacity.
- Pair validation rejects the same device, mono outputs, mismatched sample rates, and devices identified as Bluetooth.
- Cue stream errors or repeated queue underflow/overflow stop cue without stopping or redirecting master audio.
- Existing SQLite settings persist routing mode, cue-device UID, and cue delay without a migration.
- Tauri snapshots and the interim UI expose routing controls, limitation messages, and queue health.
- No production dependency was added.

A loaded 30-minute Apple M3 run used MacBook Pro Speakers for master and External Headphones for cue while repeatedly seeking and resampling mixed-rate fixtures. It completed 155,066 master callbacks and 155,068 cue callbacks. Queue depth remained between 512 and 2,048 frames, maximum relative progress deviation was one 512-frame callback quantum, and there were zero underflows, overflows, or stream errors. Loaded stability acceptance passed. Audible delay calibration and physical disconnect/reconnect checks remain required before final production acceptance.
