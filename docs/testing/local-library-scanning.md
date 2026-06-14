# Local Library Scanning Verification

## Automated Coverage

Run:

```sh
cargo test --locked library
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The scanner tests verify:

- Recursive discovery in nested folders.
- Case-insensitive supported extensions.
- Ignoring unrelated files.
- Metadata extraction with the existing decoder.
- Indexing corrupt files with fallback metadata.
- Missing-file detection after a complete rescan.
- Restoration of an existing track record.

## Manual Workflow

Run:

```sh
npm run tauri dev
```

1. Select **Add music folder**.
2. Choose a small test folder containing supported audio and nested folders.
3. Confirm the status summary and library rows match the folder contents.
4. Close and reopen the app; confirm indexed rows reload from SQLite.
5. Remove one test file outside the app, select the same folder again, and confirm the row becomes **Missing**.
6. Restore the file, rescan, and confirm the row returns to **Ready**.
7. Confirm source files and folder contents were not modified by the app.

If a folder cannot be fully read, the UI reports traversal errors and the scanner deliberately skips missing-file reconciliation for that run.
