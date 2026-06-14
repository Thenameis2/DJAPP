# ADR-004: Tauri React Application Scaffold

- Status: Approved and implemented
- Date: 2026-06-14
- Scope: Initial macOS desktop shell and frontend toolchain

## Decision

Keep the existing root Rust package as the reusable audio, decoding, mixing, and persistence engine. Add:

- A thin Tauri 2 desktop crate in `src-tauri` that depends on the root engine by local path.
- A React and TypeScript frontend in `src-ui` built by Vite.
- A narrow Tauri command boundary rather than exposing engine internals directly.
- SQLite startup in Tauri's macOS application-data directory.

## Locked Direct Dependencies

- `@tauri-apps/api` 2.11.0
- `@tauri-apps/cli` 2.11.2
- React and React DOM 19.2.7
- Vite 8.0.16
- TypeScript 6.0.3
- `@vitejs/plugin-react` 6.0.2
- `@types/react` 19.2.17
- `@types/react-dom` 19.2.3
- Rust `tauri` 2.11.2 as resolved in `src-tauri/Cargo.lock`
- Rust `tauri-build` 2.6.2 as resolved in `src-tauri/Cargo.lock`

The lockfile pins `alloc-stdlib` 0.2.2 because 0.2.3 selected `alloc-no-stdlib` 3.0.0 while Brotli 8.0.3 directly uses the incompatible 2.x trait definitions.

## Runtime Boundary

- Tauri creates the app-data directory and starts one `PersistenceWorker` owning `djapp.sqlite`.
- The initial `engine_status` command is read-only and verifies the frontend-to-Rust bridge.
- No audio engine is opened automatically at application startup.
- No library scanning, file permissions, deck transport commands, or analysis jobs are exposed yet.
- The placeholder theme toggle is session-only; SQLite theme persistence remains future work.

## UI Boundary

The initial screen is a responsive scaffold containing application status and labeled placeholders for decks A, B, and the mixer. It establishes neither final control placement nor final branding. Any production layout or visual-language decision remains subject to owner approval.

## Security

- Only Tauri core default permissions are enabled for the main window.
- At scaffold completion, no shell, network, filesystem, dialog, telemetry, or updater plugin was installed. ADR-005 later added the dialog plugin with open-only permission.
- The frontend invokes only the explicitly registered `engine_status` command.

## Implementation Result

The owner approved the application scaffold on 2026-06-14. The root engine remains independently testable, while the desktop shell and frontend have separate lockfiles and build commands.
