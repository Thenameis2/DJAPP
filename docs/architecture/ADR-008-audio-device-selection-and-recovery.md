# ADR-008: Audio Device Discovery, Selection, and Recovery

- Status: Approved and implemented
- Date: 2026-06-14
- Scope: macOS master-output discovery, persisted selection, stream replacement, and device-error recovery

## Context

The shared two-deck engine initially opened only the current macOS default output. Version 1 requires selection of built-in, wired, Bluetooth, aggregate, and compatible external outputs exposed by macOS. Device changes and disconnections must not crash the application or create competing deck streams.

## Decision

- Use the approved CPAL 0.18.1 adapter for output enumeration and stream creation.
- Persist CPAL's serialized CoreAudio `DeviceId` under the existing SQLite setting key `audio.output_device_id`.
- Keep device selection serialized on the existing mixer-service thread.
- Treat a device change as a controlled engine restart, never as audio-callback mutation.
- Before restart, capture each loaded deck's path, output-rate position, and playing state plus mixer gains.
- Close the old engine, open one stream on the selected device, reload tracks, seek, restore gains, and resume decks that were playing.
- If a saved or failed selected device cannot open, attempt the macOS default output and report the fallback.
- Detect callback stream errors during the existing 2 Hz snapshot cycle and use the same restart path.
- Refresh the UI device list every three seconds to reflect connections and removals.
- Add no dependency and make no database-schema change.

## Identity And Capability Limits

CPAL's CoreAudio backend exposes the macOS device UID as a persistent `DeviceId`, which is preferable to matching display names. CPAL currently reports reliable name, default format, channels, and aggregate-device classification, but macOS interface classification may remain `unknown` for some Bluetooth devices. The UI therefore displays Bluetooth/wireless latency guidance beside the selector rather than claiming perfect transport detection.

This milestone selects only the stereo master output. Independent headphone cue routing still requires a separate approved design based on available channels or multiple devices.

## Tauri Contract

- `audio_output_devices` returns output UID, name, default status, interface classification, channels, and default sample rate.
- `audio_select_output_device` switches the shared engine and persists the UID only after a successful selection.
- `mixer_snapshot` includes active output UID/name, recovery count, and a visible device message.

## Real-Time Rules

- Enumeration, persistence, decoder reload, and stream construction remain outside the callback.
- The callback continues to use only fixed render state, lock-free queues, and atomic health counters.
- Device changes are serialized with transport commands by the mixer owner.
- Only one application output stream is active after each transition.

## Verification

- Device enumeration must return stable UIDs and usable default configurations on the Apple M3.
- Selecting the active output while both mixed-rate decks play must restore both play states and advance both clocks.
- Missing saved devices must fall back to the macOS default with a visible message.
- Stream errors must not panic the app and must enter the controlled recovery path.
- Automated tests, Clippy, frontend build, Tauri bundle, and direct CoreAudio smoke tests must pass.

## Approval

The owner approved audio-device discovery, output selection, and stream recovery on 2026-06-14. This does not approve independent headphone cue routing, aggregate-device creation, split-output mode, or automatic changes to macOS system audio settings.
