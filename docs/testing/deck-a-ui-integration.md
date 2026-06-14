# Deck A UI Integration Verification

## Automated Checks

```sh
cargo test --locked
cargo test --manifest-path src-tauri/Cargo.toml --locked
npm run build
```

The Tauri tests verify that an unloaded service reports paused state and rejects transport commands without opening audio hardware.

## CoreAudio Smoke Test

Run with direct audio-device access:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --locked \
  hardware_transport_commands_complete_without_audio_errors -- --ignored --nocapture
```

The test uses zero gain, loads a 48 kHz fixture, plays, pauses, seeks to 1 second with resume, and stops. On the Apple M3 test machine it completed 61 callbacks with zero underflows, recycling failures, stream errors, or worker errors. Sixteen stale blocks were correctly discarded after generation changes.

## Manual UI Workflow

```sh
npm run tauri dev
```

1. Scan the fixture folder or another small local music folder.
2. Select **Load A** on a ready track.
3. Confirm title, duration, and paused state appear in deck A.
4. Test play, pause, position seeking, and stop.
5. Confirm the position clock advances while playing and stops while paused.
6. Confirm a missing or corrupt track reports an error without crashing the app.
7. Confirm deck health continues to show zero stream errors during normal playback.

The application currently uses the default macOS output device. Output selection and independent cue routing are not part of this milestone.
