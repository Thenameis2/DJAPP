# Dual-Device Cue Routing Test

## Automated Verification

Run:

```sh
cargo test --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --all-targets -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
npm run build
```

The direct CoreAudio test is ignored by default:

```sh
cargo test --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::dual_device_cue_runs_loaded_two_deck_audio \
  -- --ignored --nocapture
```

It requires connected `MacBook Pro Speakers` and `External Headphones` routes.
Set `DJAPP_DUAL_CUE_RUN_SECONDS` to control its duration. During longer runs the harness seeks both fixtures every 1.5 seconds so both decoder and resampler pipelines remain active.

## Short Apple M3 Result

Date: 2026-06-14

- Loaded 48 kHz and 96 kHz WAV fixtures into separate decks and resampled both to the 44.1 kHz engine rate.
- Used MacBook Pro Speakers for stereo master and External Headphones for stereo cue.
- Ran both decks for three seconds at zero channel and cue gain.
- Master callbacks: 261.
- Cue callbacks: 263.
- Cue rendered frames: 134,656.
- Cue queue depth: 1,024 frames at snapshot.
- Observed cue queue range: 512-2,048 frames.
- Master stream errors: 0.
- Cue stream errors: 0.
- Cue underflow callbacks: 0.
- Cue overflow callbacks: 0.

## Audible-Signal Diagnostic

Date: 2026-06-14

The CoreAudio acceptance harness now records the peak amplitude actually consumed by the cue-device callback. A five-second run used low nonzero deck and cue gains, moved the crossfader left to select Deck B automatically, and measured a cue peak of `0.00062491273`. It reported zero cue underflows, overflows, or stream errors.

This verifies that decoded Deck B audio reaches the External Headphones callback. The UI exposes Cue Level, Cue/Master blend, and a live cue-signal indicator. If the indicator is active but no sound is audible, check the macOS output volume for External Headphones.

## Extended Acceptance

### Loaded 30-Minute Result

Date: 2026-06-14

Command:

```sh
DJAPP_DUAL_CUE_RUN_SECONDS=1800 cargo test \
  --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::dual_device_cue_runs_loaded_two_deck_audio \
  -- --ignored --nocapture
```

- Duration: 1,800 seconds.
- Master callbacks: 155,066.
- Master rendered frames: 79,393,792.
- Cue callbacks: 155,068.
- Cue rendered frames: 79,394,816.
- Final master/cue relative frame difference: 1,024 frames, matching buffered queue depth.
- Maximum relative deviation from the post-start baseline: 512 frames, one callback quantum.
- Cue queue range: 512-2,048 frames.
- Master stream errors: 0.
- Cue stream errors: 0.
- Cue underflow callbacks: 0.
- Cue overflow callbacks: 0.

Result: loaded dual-device stability acceptance passed. The queue remained bounded with no accumulating trend while both decoder pipelines repeatedly sought and resampled mixed-rate fixtures.

### Manual Acceptance Result

On 2026-06-15, the owner confirmed that the cue signal is audible and the target speaker/headphone setup works correctly. ADR-010 is hardware accepted.

The acceptance covered:

- Audible separate headphone cue.
- Cue A/B and crossfader-follow automatic cue.
- Cue level and cue/master monitoring controls.
- MacBook Pro Speakers as master and External Headphones as cue.
- Live cue-signal telemetry with no reported stream, queue, or privacy failure.
