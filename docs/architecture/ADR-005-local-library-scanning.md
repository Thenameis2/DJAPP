# ADR-005: Local Library Folder Selection And Scanning

- Status: Approved and implemented
- Date: 2026-06-14
- Scope: Native folder selection, recursive local scanning, metadata indexing, and library display

## Decision

- Add `tauri-plugin-dialog` 2.7.1 and `@tauri-apps/plugin-dialog` 2.7.1 for the native macOS folder picker.
- Grant only `dialog:allow-open` to the main window.
- Keep all recursive traversal and media inspection in the existing Rust engine.
- Use the standard library for traversal and the approved Symphonia decoder boundary for metadata.
- Reuse SQLite schema version 1; no table or migration change is required.
- Run scans through Tauri's blocking worker pool, never on the UI or audio callback thread.

The dialog plugin includes `tauri-plugin-fs` transitively, but this application grants no filesystem-plugin capability and does not call its frontend API.

## Supported Files

Extension matching is case-insensitive for:

- MP3
- WAV
- FLAC
- AAC
- M4A
- AIF
- AIFF

Directories are scanned recursively. Symbolic links are ignored to avoid cycles and unexpected traversal outside the selected tree. Unsupported files are ignored without modifying them.

## Persistence Behavior

- Canonical selected folder paths are upserted into `library_roots`.
- Tracks are identified by their unique canonical path and compared by file size and modification time.
- Decodeable tracks store normalized title, artist, duration, sample rate, channel count, and codec.
- Corrupt or unsupported-content files with a supported extension remain indexed using filename and extension fallback metadata.
- A complete rescan marks previously indexed tracks as missing when their paths are absent.
- If any directory cannot be read, missing-file reconciliation is skipped to avoid false missing records.
- Restored files retain the same track row and user-authored data.

## Tauri Contract

- `scan_music_folder(path: String) -> ScanResult`
- `library_tracks() -> TrackView[]`

The UI uses the dialog plugin to obtain a user-selected path, then invokes the scanner. The library table is an interim browsing surface, not the final approved DJ layout.

## Security And Privacy

- No source file is written, moved, renamed, or deleted.
- No network service, account, telemetry, or cloud storage is involved.
- The frontend has no general filesystem read/write capability.
- The selected path and indexed metadata remain local in SQLite.

## Implementation Result

The owner approved folder selection and recursive scanning on 2026-06-14. Automated tests cover nested folders, case-insensitive extension matching, ignored non-audio files, corrupt media, missing-file detection, and restoration.
