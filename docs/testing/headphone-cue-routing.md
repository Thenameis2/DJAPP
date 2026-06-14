# Headphone Cue Routing Testing

## Automated Checks

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm run build
npm run tauri build -- --debug
```

Deterministic tests verify master channels 1–2, pre-crossfader cue channels 3–4, equal-power cue/master blend, cue gain, simultaneous Cue A/B summing, silence on unused channels, and unchanged stereo master-only behavior.

## Current Mac Smoke Test

```sh
cargo test --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::mixed_rate_two_deck_commands_share_one_healthy_stream \
  -- --ignored --nocapture
```

With wired headphones connected, External Headphones and MacBook Pro Speakers each report two channels. The test requires `master-only` routing, rejects active cue commands, and confirms two-deck playback and device restart remain healthy.

## Four-Channel Acceptance

Connect a four-channel audio interface or select a macOS aggregate device exposing at least four output channels.

1. Confirm the output selector reports `4ch max` or greater.
2. Load both decks and enable Cue A, Cue B, and both together.
3. Verify master is present only on outputs 1–2 and cue is present only on outputs 3–4.
4. Move the crossfader and confirm it changes master but not the selected pre-crossfader cue deck.
5. Verify cue gain and cue/master blend endpoints and center.
6. Switch away from the device and confirm master continues while cue becomes disabled if the next device has fewer than four channels.
7. Require zero underflows, recycling failures, stream errors, and worker errors.

Physical four-channel acceptance is pending because that hardware is not currently available.
