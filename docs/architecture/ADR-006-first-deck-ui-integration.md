# ADR-006: First Deck UI Integration

- Status: Approved and implemented
- Date: 2026-06-14
- Scope: First functional UI-to-audio vertical slice

## Recommendation

Connect library tracks to deck A before expanding the visual design or adding more DSP.

The approved implementation would:

- Keep one long-lived audio-engine owner in Tauri state.
- Add commands to load a local indexed track into deck A, play, pause, seek, stop, and read a low-frequency status snapshot.
- Validate that requested paths exist in the indexed SQLite library before opening them.
- Keep callback commands on the existing bounded real-time queue.
- Add basic functional controls inside the current placeholder, without establishing the final production layout.
- Surface device, decoder, missing-file, and transport errors without crashing the app.
- Avoid adding production dependencies or changing the database schema.

## Acceptance Checks

- A library row can load into deck A and play through the selected default output.
- Play, pause, seek, and stop work from React.
- UI polling does not block or allocate in the audio callback.
- Missing and corrupt indexed files produce visible errors.
- Existing CLI, scanner, persistence, and audio tests remain green.
- A direct Apple M3 playback smoke test reports healthy callback counters.

Approval authorizes this deck-A vertical slice within the current scaffold. It does not approve the final two-deck screen design, new DSP, output routing changes, or additional dependencies.

## Implementation Result

The owner approved this ADR on 2026-06-14.

- Added a dedicated `djapp-deck-a-service` thread as the sole owner of `DeckTransport` and its CoreAudio stream.
- Added indexed-track validation before opening media; missing and nonexistent paths are rejected with user-facing errors.
- Added Tauri commands for load, play, pause, seek, stop, and 2 Hz snapshots.
- Replaced only the existing deck A placeholder with functional controls and added **Load A** actions to the interim library table.
- Added output sample rate to `DeckMediaInfo` so mixed-rate UI position clocks use rendered frames correctly.
- Added no dependencies and made no schema changes.

The layout remains an interim functional surface. Deck B, mixer controls, final visual design, output routing, and additional DSP remain outside this approval.
