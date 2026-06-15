# Shared Two-Deck UI Verification

## Automated Checks

Run from the repository root:

```sh
cargo fmt --all --manifest-path src-tauri/Cargo.toml -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
npm run build
npm run tauri build -- --debug
```

## CoreAudio Smoke Test

This test needs direct access to the macOS output device:

```sh
cargo test --manifest-path src-tauri/Cargo.toml \
  mixer_service::tests::mixed_rate_two_deck_commands_share_one_healthy_stream \
  -- --ignored --nocapture
```

The test loads 48 kHz WAV on Deck A and 96 kHz WAV on Deck B, mutes both channel gains, starts both decks, moves the crossfader, seeks Deck B, and stops Deck A. It requires one positive shared callback count and zero per-deck underflows, recycling failures, stream errors, or worker errors.

## Manual UI Check

1. Scan a folder containing supported local audio files.
2. Load any ready track into Deck B before Deck A and confirm it succeeds.
3. Load tracks into both decks and use play, pause, seek, and stop independently.
4. Move Deck A gain, Deck B gain, master gain, and the crossfader.
5. Confirm the health counters continue updating and errors remain zero.
## 2026-06-14 Visual Revision

The owner-approved interface direction uses an original professional DJ layout with:

- Two large deck surfaces with track headers, progress waveforms, platter visualization, transport controls, gain, and simplified EQ/filter placeholders.
- A compact center mixer and a horizontal crossfader below both decks.
- Audio-device and cue-routing controls in a collapsible top panel.
- A dense lower local-library browser and a clearly marked future AutoMix panel.
- Light and dark theme support without copied third-party branding or visual assets.

## Automatic Cue Behavior

- Crossfader below `-0.05`: Deck A is on master and Deck B is automatically cued.
- Crossfader above `0.05`: Deck B is on master and Deck A is automatically cued.
- Crossfader from `-0.05` through `0.05`: both decks are on master and automatic cue is cleared.
- Manual Cue A/B controls may override the current selection until the crossfader moves again.
- Cue remains capability-gated and can never be routed into a two-channel master output.

## Verification

```sh
cargo test --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
npm run build
```

Manual visual review should cover laptop sizing, long track names, empty library state, both themes, disabled cue routing, and separate-device cue health text.
