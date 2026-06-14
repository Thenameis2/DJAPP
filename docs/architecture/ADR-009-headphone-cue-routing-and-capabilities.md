# ADR-009: Headphone Cue Routing And Hardware Capabilities

- Status: Approved and implemented; four-channel hardware acceptance pending
- Date: 2026-06-14
- Scope: Stereo master/cue capability detection, channel routing, cue selection, and cue/master blend

## Context

The application currently renders one stereo master mix to a selected CoreAudio output. DJ headphone cue requires an independent private signal that can monitor Deck A, Deck B, or both before the crossfader, with a cue/master blend.

With no headphones connected, the target Apple M3 exposes:

- MacBook Pro Speakers: 2 output channels at 44.1 kHz.
- Microsoft Teams Audio: 1 output channel at 48 kHz.

With wired headphones connected, CoreAudio instead exposes:

- External Headphones: 2 output channels at 44.1 kHz and selected as the macOS default.
- MacBook Pro Speakers: a separate 2-channel output at 44.1 kHz.
- Microsoft Teams Audio: 1 output channel at 48 kHz.

The connected headphones therefore create two separately addressable stereo output devices on this Mac. This makes a built-in-speakers master plus external-headphones cue configuration technically testable, but it does not provide one four-channel stream or prove that the two device clocks remain synchronized during a DJ set.

## Options

### One Four-Channel Device Or Aggregate Device

Use one CoreAudio stream and one hardware clock. Route stereo master to output channels 1–2 and stereo cue to channels 3–4.

- Strengths: deterministic synchronization, no cross-device drift, one callback, compatible with multichannel interfaces/controllers and user-created macOS aggregate devices.
- Costs: requires compatible hardware or an aggregate device configured outside the app.
- Recommendation: implement first.

### Separate Master And Cue Devices

Open one stream per device and distribute the same engine timeline to both.

- Strengths: could pair built-in speakers with a separately exposed headphone or USB output.
- Costs: independent clocks drift; requires asynchronous resampling, buffering, latency alignment, more complex recovery, and substantially broader hardware testing.
- Updated target observation: the connected wired headphones and MacBook Pro Speakers are separately addressable stereo devices with matching nominal 44.1 kHz rates.
- Recommendation: run a focused dual-stream synchronization and latency spike before deciding whether this mode can join version 1. Matching nominal sample rates do not guarantee a shared clock or matched output latency.

### Mono Split Output

Send mono master on one stereo channel and mono cue on the other through a splitter cable.

- Strengths: works with a two-channel output and inexpensive hardware.
- Costs: loses stereo master and stereo cue, requires a physical splitter, is easy to misconfigure, and is below the preferred version-one quality bar.
- Recommendation: defer as an optional fallback requiring separate product approval.

### Software-Created Aggregate Device

Create or alter aggregate devices through CoreAudio APIs.

- Strengths: could reduce manual Audio MIDI Setup work.
- Costs: changes system-level audio configuration, raises permission/support risk, and is more invasive than selecting an existing aggregate device.
- Recommendation: do not create aggregate devices in this milestone; document how users can configure one in macOS.

## Recommendation

Implement capability-gated cue routing on a single selected output device with at least four channels.

- Master channel pair: output channels 1–2.
- Cue channel pair: output channels 3–4.
- Devices with more than four channels use the first four channels initially; configurable channel-pair mapping is deferred.
- Deck cue taps are post-deck gain and pre-crossfader.
- Cue A and Cue B may be enabled independently; enabling both sums them with bounded equal gain.
- Cue/master blend ranges from cue-only through an equal-power center to master-only.
- A cue gain protects headphone level independently from master gain.
- The selected device continues to use one stream, one callback, and one master clock.
- Two-channel devices remain fully usable for stereo master, but cue controls are disabled with a clear capability explanation.
- Persist cue A, cue B, cue/master blend, and cue gain in the existing `settings` table; no schema migration is needed.
- Add no production dependency.

In parallel, the newly available speaker/headphone pair justifies a separately approved experimental spike. That spike should open silent synchronized master and cue streams, measure callback cadence and relative frame progress for at least 30 minutes, exercise disconnect/reconnect, and determine whether CoreAudio keeps these built-in routes on a sufficiently common clock. It must not be presented as production cue support until those measurements pass agreed limits.

## Capability Model

Extend output-device information with:

- Default output-channel count.
- Maximum supported output-channel count.
- `stereo_master_supported`: at least two channels.
- `stereo_cue_supported`: at least four channels in a usable stream configuration.
- `routing_mode`: `master-only` or `master-and-cue`.
- Human-readable limitation or setup guidance.

The engine must choose a supported four-channel configuration explicitly when cue is enabled. The device's default configuration alone is not sufficient if another supported configuration exposes more channels.

## Engine Design

- Extend the mixer callback to render stereo Deck A and Deck B frames once per engine frame.
- Produce the existing crossfaded master mix independently of cue selection.
- Produce the cue bus from selected pre-crossfader deck signals.
- Write master left/right to channels 0/1 and cue left/right to channels 2/3.
- Fill unused output channels with silence.
- Keep cue flags, blend, and gain in the existing lock-free mixer command queue.
- Keep capability discovery, configuration selection, persistence, and error handling outside the callback.

For master-only devices, preserve the current stereo callback behavior exactly.

## Service And UI Contract

Proposed Tauri commands:

- `deck_a_set_cue(enabled)`
- `deck_b_set_cue(enabled)`
- `mixer_set_cue_blend(value)` where `-1` is cue-only and `1` is master-only
- `mixer_set_cue_gain(gain)`

Combined mixer snapshots add cue selections, blend, gain, routing mode, and limitation text.

The current mixer area gains compact cue toggles and blend/gain controls. They remain visible but disabled when the selected output lacks four-channel routing, so the limitation is discoverable rather than hidden. This is a functional extension to the interim UI, not final visual-design approval.

## Recovery Rules

- Device changes use the ADR-008 controlled restart.
- Restore cue settings after rebuilding the stream.
- If a device change reduces available channels below four, continue stereo master playback, disable cue routing, and show the limitation.
- Never fail master playback solely because cue routing is unavailable.
- Never silently send private cue audio into the public master channels.

## Testing

- Deterministic callback tests verify channel mapping, cue selection, pre-crossfader behavior, blend endpoints/center, gain clamping, silence on unused channels, and master-only regression behavior.
- Capability tests cover 1-, 2-, 4-, and 8-channel configuration sets.
- Service tests verify settings survive a controlled device restart and cue disables safely when routing capability is lost.
- Apple M3 hardware smoke confirms the current two-channel output reports master-only and does not expose active cue controls.
- A true four-channel hardware or aggregate-device acceptance test remains required before declaring headphone cue production-ready.

## Tradeoffs

This recommendation delivers correct professional routing when the hardware can support it and tells the truth when it cannot. It does not make headphone cue work on the current built-in two-channel output. Supporting that without additional hardware would require the deferred mono-split compromise or a separately designed multi-device clocking system.

The connected wired headphones improve the available test setup: separate-device cue may be practical on this specific Mac, but the architecture still needs measured clock and latency behavior before adopting two-stream routing.

## Approval Request

Approval authorizes:

- Single-stream four-channel master/cue routing.
- Capability detection and clear master-only fallback.
- Cue A/B, cue/master blend, and cue gain controls.
- Persistence through the existing settings table.
- Tests and documentation with no new dependency or schema migration.

Approval does not authorize separate-device routing, automatic aggregate-device creation, mono split output, configurable channel-pair mapping, or final UI design.

A separate approval may authorize a non-production dual-stream speaker/headphone synchronization spike using the currently connected devices.

## Implementation Result

The owner approved ADR-009 on 2026-06-14.

- Output discovery now reports default and maximum channels, stereo-master support, stereo-cue support, routing mode, and limitation guidance.
- The engine selects a supported four-channel configuration when one exists; otherwise it preserves the normal default configuration.
- The callback renders stereo decks once, sends master to channels 1–2, cue to channels 3–4, and silences unused channels.
- Cue taps are post-deck gain and pre-crossfader. Cue A/B, equal-power cue/master blend, and independent cue gain use the lock-free mixer command queue.
- Cue preferences use existing SQLite settings without a schema migration and are restored across stream rebuilds.
- The interim mixer UI exposes cue controls but disables them with a clear explanation on master-only devices.
- Current External Headphones and MacBook Pro Speakers routes are both correctly classified as master-only. Separate-device routing remains outside this approval.
- Deterministic channel-routing tests pass. A four-channel interface or user-created aggregate device is still required to complete physical channels 3–4 acceptance.

## Dual-Stream Spike Result

The owner separately approved the non-production speaker/headphone synchronization spike on 2026-06-14. A silent 30-minute run opened MacBook Pro Speakers and External Headphones concurrently at 44.1 kHz. It reported zero accumulated sampled drift, a maximum one-quantum observation difference of 512 frames, and zero stream errors.

This supports a future production proposal for these separately exposed routes, but does not authorize it. Fixed output-latency difference, real mixer load, bounded shared buffering, and disconnect/reconnect recovery still need design and validation.
