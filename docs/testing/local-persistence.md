# Local Persistence Verification

## Scope

Schema version 1 is implemented in `src/persistence.rs` with `rusqlite` 0.40.1 and bundled SQLite. A dedicated worker thread owns the production connection; audio callbacks and decoder workers do not access SQLite.

## Database Configuration

- Foreign keys enabled.
- WAL journal mode requested for file databases.
- Normal synchronous mode.
- Two-second busy timeout.
- UTC Unix millisecond timestamps.
- `PRAGMA user_version = 1` after successful migration.

## Automated Coverage

Run:

```sh
cargo test --locked persistence
```

Coverage includes:

- Fresh schema creation and rejection of databases newer than the application.
- Transactional rollback when a migration statement fails.
- Settings and queue order surviving worker shutdown and database reopen.
- Inserted, unchanged, modified, missing, and restored track states.
- Loop constraints and foreign-key cascades.
- Re-analysis preserving user-authored corrections and cues.

## Current Boundary

The persistence worker accepts a database path from its caller. Selecting the macOS application-data directory, backup/reset UI, library scanning, and cache-file creation remain future application-layer work.
