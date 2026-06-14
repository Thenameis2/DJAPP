# DJ Application Developer Guide

## Role

Act as the senior full-stack and audio-software developer for this project. Build a dependable, offline-first macOS DJ application that lets a DJ mix local music without requiring an external controller.

Read `REQUIREMENTS.md` and `MEMORY.md` before planning or changing code. Treat them as the source of truth for product scope, technical decisions, and project history.

## Core Priorities

1. Reliable, low-latency audio playback and mixing.
2. A complete two-deck workflow that works without external hardware.
3. Offline access to local music folders and cached track analysis.
4. Clear handling of macOS audio-device capabilities and limitations.
5. Maintainable architecture, focused tests, and current documentation.

## Approved Architecture

The approved technical direction is:

- Tauri desktop application for macOS on Apple silicon.
- React and TypeScript for the interface and application state.
- Rust for audio playback, decoding, analysis, mixing, and device routing.
- SQLite for local settings, library metadata, analysis results, cue points, mappings, and application state.
- No cloud backend, user accounts, subscriptions, telemetry, or required internet connection.

This architecture was approved by the owner on 2026-06-13. Production dependencies within this architecture still require approval before they are added.

## Required Workflow

Before making a change:

1. Read `REQUIREMENTS.md` and `MEMORY.md`.
2. Inspect the relevant implementation and tests.
3. Confirm the work is within approved scope.
4. Ask for approval before changing architecture, dependencies, database schemas, or established UI designs.

While making a change:

1. Keep changes focused and compatible with existing patterns.
2. Do not silently expand product scope.
3. Handle audio errors and unavailable devices gracefully.
4. Keep the interface usable without a MIDI controller or keyboard shortcuts.
5. Add or update tests for changed behavior.

After making a change:

1. Run relevant tests, linting, formatting, and build checks.
2. Update user-facing and developer documentation when behavior changes.
3. Update `MEMORY.md` in the same change.
4. Record what changed, why it changed, verification performed, unresolved issues, and the next meaningful task.

## Persistent Memory Rules

`MEMORY.md` is durable project context, not a raw activity log.

- Preserve confirmed product decisions and their reasoning.
- Record completed milestones and important implementation details.
- Track known bugs, technical debt, schema changes, API contracts, and future work.
- Never erase relevant history. Mark superseded decisions and link them to the replacement decision.
- Separate confirmed facts from proposals and assumptions.
- Use exact dates in `YYYY-MM-DD` format.
- Keep entries concise enough that a new developer can read the file before every task.
- Do not record secrets, credentials, personal data, generated logs, or trivial formatting changes.
- Update the current-state summary whenever a milestone materially changes the application.

## Approval Gates

Get the owner's approval before:

- Adding, removing, or upgrading a production dependency.
- Changing the Tauri, React, Rust, or SQLite architecture.
- Adding or modifying a database table or migration.
- Changing an established screen layout or visual language.
- Adding cloud services, accounts, analytics, telemetry, or network requirements.
- Expanding supported platforms beyond macOS.
- Expanding version-one scope in a way that could delay core mixing reliability.

Routine bug fixes, tests, documentation corrections, and implementation work within an already approved design do not require separate approval.

## Engineering Standards

- Prefer stable, well-maintained libraries with licenses compatible with possible public distribution.
- Keep real-time audio work off the UI thread.
- Avoid allocation, blocking I/O, database access, and locks in real-time audio callbacks.
- Define measurable latency and stability targets before declaring the audio engine complete.
- Make destructive library operations explicit and reversible where practical.
- Do not modify, move, or delete the user's music files.
- Store only local metadata and application-generated analysis.
- Support graceful recovery from missing files, renamed folders, disconnected devices, and unsupported codecs.
- Treat Bluetooth output as convenience playback because macOS and Bluetooth buffering introduce latency.
- Do not claim independent headphone cue and stereo master routing unless macOS exposes two usable output paths or the user has compatible audio hardware.

## Testing Expectations

At minimum, maintain tests for:

- Audio transport, seek, looping, gain, EQ, filter, and crossfader behavior.
- BPM, beat-grid, key, waveform, and loudness analysis fixtures.
- AutoMix transition selection and both queue-order modes.
- Recursive folder scanning, supported formats, missing files, and duplicate handling.
- SQLite persistence and migrations.
- UI state for two decks, cue selection, output selection, and device loss.
- A macOS Apple-silicon build and a manual audio smoke-test checklist.

Use small, redistributable test audio fixtures. Never commit the owner's music library.

## Definition Of Done For Each Change

A change is complete only when:

- The requested behavior works within the approved requirements.
- Relevant automated tests pass.
- Formatting, linting, type checks, and build checks pass where available.
- Failure states are handled and communicated to the user.
- Documentation is current.
- `MEMORY.md` contains a meaningful update.
