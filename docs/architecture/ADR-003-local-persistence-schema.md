# ADR-003: Local Persistence And Initial Schema

- Status: Approved and implemented
- Date: 2026-06-13
- Scope: Local SQLite dependency, ownership model, migrations, and version-one schema

## Context

The application must persist settings, selected music folders, indexed tracks, analysis results, beat-grid corrections, hot cues, loops, queue state, and audio preferences without accounts or cloud services.

Database work must never run in the CoreAudio callback. The schema must support versioned analysis and migrations so improved algorithms can invalidate derived data without deleting user-authored cues or corrections.

## Dependency Recommendation

Approve `rusqlite` 0.40.1 with the `bundled` feature only.

- License: MIT; SQLite is public domain.
- Bundled SQLite: predictable packaging and version across Macs.
- Current upstream guidance recommends `bundled` for applications controlling their own databases.
- No async runtime, connection pool, ORM, serialization, or time/date dependency is needed initially.

Source: [rusqlite repository and usage guidance](https://github.com/rusqlite/rusqlite).

## Ownership Model

- One persistence worker owns the SQLite connection.
- Commands and results cross typed non-real-time channels.
- Audio callbacks, decoder workers, and analysis workers never access the connection directly.
- Enable foreign keys, WAL journal mode, a bounded busy timeout, and normal synchronous mode.
- Use short explicit transactions and prepared statements.
- Store timestamps as UTC Unix milliseconds.
- Store filesystem paths as UTF-8 display text plus platform file identity fields when available later.

## Migration Strategy

- Keep ordered SQL migrations embedded in the Rust binary.
- Use SQLite `PRAGMA user_version` as the schema version.
- Run every migration in a transaction before application services start.
- Back up or copy the database before destructive migrations once public distribution begins.
- Never silently downgrade a newer database.
- Test fresh creation and upgrades from every released schema version.

## Proposed Version-One Schema

### `settings`

- `key TEXT PRIMARY KEY`
- `value TEXT NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

Stores theme, output preference, cue routing preference, AutoMix defaults, and other application settings. Values use documented per-key formats; secrets are forbidden.

### `library_roots`

- `id INTEGER PRIMARY KEY`
- `path TEXT NOT NULL UNIQUE`
- `display_name TEXT`
- `recursive INTEGER NOT NULL DEFAULT 1 CHECK (recursive IN (0, 1))`
- `enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1))`
- `last_scan_at_ms INTEGER`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

### `tracks`

- `id INTEGER PRIMARY KEY`
- `library_root_id INTEGER REFERENCES library_roots(id) ON DELETE SET NULL`
- `path TEXT NOT NULL UNIQUE`
- `file_size INTEGER NOT NULL`
- `modified_at_ms INTEGER NOT NULL`
- `content_fingerprint TEXT`
- `title TEXT`
- `artist TEXT`
- `album TEXT`
- `genre TEXT`
- `duration_frames INTEGER`
- `sample_rate INTEGER`
- `channels INTEGER`
- `codec TEXT`
- `missing INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1))`
- `created_at_ms INTEGER NOT NULL`
- `updated_at_ms INTEGER NOT NULL`

Index `library_root_id`, `artist`, `title`, and `missing`. Original music files remain untouched.

### `track_analysis`

- `track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE`
- `analysis_version INTEGER NOT NULL`
- `status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'failed'))`
- `bpm REAL`
- `bpm_confidence REAL`
- `musical_key TEXT`
- `key_confidence REAL`
- `integrated_lufs REAL`
- `true_peak_db REAL`
- `beat_grid_path TEXT`
- `waveform_path TEXT`
- `error_message TEXT`
- `analyzed_at_ms INTEGER`

Large waveform and beat-grid caches stay in versioned files outside SQLite. The database stores their paths and analysis version.

### `track_corrections`

- `track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE`
- `bpm REAL`
- `musical_key TEXT`
- `beat_grid_offset_frames INTEGER`
- `updated_at_ms INTEGER NOT NULL`

User corrections remain separate from generated analysis so re-analysis cannot overwrite them.

### `hot_cues`

- `id INTEGER PRIMARY KEY`
- `track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE`
- `slot INTEGER NOT NULL CHECK (slot >= 0)`
- `position_frames INTEGER NOT NULL CHECK (position_frames >= 0)`
- `label TEXT`
- `color TEXT`
- `updated_at_ms INTEGER NOT NULL`
- `UNIQUE(track_id, slot)`

### `saved_loops`

- `id INTEGER PRIMARY KEY`
- `track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE`
- `slot INTEGER NOT NULL CHECK (slot >= 0)`
- `start_frame INTEGER NOT NULL CHECK (start_frame >= 0)`
- `end_frame INTEGER NOT NULL CHECK (end_frame > start_frame)`
- `label TEXT`
- `updated_at_ms INTEGER NOT NULL`
- `UNIQUE(track_id, slot)`

The cross-column loop constraint must be implemented as `CHECK (end_frame > start_frame)` in the table definition.

### `queue_items`

- `id INTEGER PRIMARY KEY`
- `track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE`
- `position INTEGER NOT NULL UNIQUE CHECK (position >= 0)`
- `added_at_ms INTEGER NOT NULL`

Stores the current local queue only. AutoMix scoring remains derived runtime state.

## Deferred Tables

- MIDI controller mappings.
- Play history and recording history.
- Multiple named playlists or crates.
- Cloud/account state.
- Analytics or telemetry.

These are excluded until requirements justify them.

## Data Location And Recovery

- Use the macOS application-data directory selected by Tauri for the database and cache root.
- Keep the database, waveform cache, and beat-grid cache in separate documented paths.
- Provide future backup/reset commands that close the persistence worker before copying or deleting local data.
- Removing a library root or track record must never delete the source music file.

## Acceptance Tests

- Fresh schema creation sets the expected `user_version`.
- Every foreign key and uniqueness constraint is exercised.
- Settings and queue order survive reopen.
- Track upsert detects unchanged, modified, missing, and restored files.
- Re-analysis updates generated data without changing corrections, cues, or loops.
- Cascade behavior removes derived track records but never source files.
- Migration rollback leaves the previous schema usable.
- Persistence work under load does not affect audio callback counters.

## Approval Requested

Approval authorizes adding `rusqlite` 0.40.1 with `bundled`, implementing the persistence worker, and creating schema version 1 exactly as described. Any schema change after approval requires another owner approval and migration update.

## Implementation Result

The owner approved this ADR on 2026-06-13.

- Added `rusqlite` 0.40.1 with bundled SQLite.
- Added schema version 1 as an embedded transactional migration.
- Added a single-owner persistence worker with typed request and response channels.
- Added settings, library-root, track-upsert, analysis, missing-file, and queue operations.
- Added tests for fresh creation, newer-schema rejection, migration rollback, reopen persistence, track file-state changes, constraints, cascades, and preservation of user corrections during re-analysis.
- Set the project minimum Rust version to 1.96 because `libsqlite3-sys` 0.38.1 uses compiler support absent from the previously installed Rust 1.85 toolchain.

No Tauri application-data path is wired yet because the Tauri shell has not been scaffolded. Callers currently provide the database path when starting the worker.
