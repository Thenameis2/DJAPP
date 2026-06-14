# Application Scaffold Verification

## Automated Checks

Run the frontend build:

```sh
npm run build
```

Run the engine tests and checks:

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

Run the Tauri shell checks:

```sh
cargo check --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
```

## Manual Launch Check

```sh
npm run tauri dev
```

Confirm:

1. The macOS window opens at no less than 720 by 600 pixels.
2. The status card reports that the Rust engine and SQLite schema version 1 are ready.
3. The dark and light scaffold themes switch without visual loss of labels or focus indication.
4. Closing the window exits cleanly and releases the persistence worker.
5. `djapp.sqlite` exists under the application-data directory for `com.djapp.desktop`.

The scaffold does not yet start audio, scan music, persist theme selection, or provide functional deck controls.
