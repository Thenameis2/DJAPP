# Audio Device Selection And Recovery Testing

## Automated Checks

```sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm run build
npm run tauri build -- --debug
```

## Direct CoreAudio Test

```sh
cargo test --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::mixed_rate_two_deck_commands_share_one_healthy_stream \
  -- --ignored --nocapture
```

The test loads 48 kHz and 96 kHz fixtures, starts both decks silently, reselects the current default output, and verifies both decks remain playing and advance beyond their pre-switch positions. It then exercises crossfading, seek, and stop while requiring zero underflows, recycling failures, stream errors, and worker errors.

## Manual Device Workflow

1. Launch the app and confirm the output selector lists the built-in output and any connected wired, Bluetooth, or aggregate devices.
2. Start both decks, change the output, and confirm playback resumes near the previous positions.
3. Quit and relaunch; confirm the selected output is restored from SQLite.
4. Disconnect a selected external output during playback. Confirm the app remains responsive and displays recovery or fallback status.
5. Reconnect the device and confirm it returns to the list within approximately three seconds.
6. For Bluetooth or wireless output, confirm the latency warning remains visible.
