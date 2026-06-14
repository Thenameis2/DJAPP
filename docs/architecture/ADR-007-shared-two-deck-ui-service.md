# ADR-007: Shared Two-Deck UI Audio Service

- Status: Approved and implemented
- Date: 2026-06-14
- Scope: Deck B, shared mixer ownership, crossfader, channel gain, and master gain UI integration

## Context

Deck A currently owns a standalone `DeckTransport` and CoreAudio stream. The existing `MixerEngine` already proves that two decks should share one output callback and master clock. Adding a second standalone stream would create avoidable synchronization and device-contention problems.

## Recommendation

Replace the temporary deck-A service internals with one dedicated service thread that owns a shared two-deck engine.

- Preserve the current deck-A Tauri command names where practical.
- Allow either deck to be unloaded initially; this requires adapting the existing mixer pipeline rather than opening placeholder media.
- Load only indexed, non-missing SQLite tracks.
- Add deck B load, play, pause, seek, stop, and snapshot commands.
- Add crossfader, per-deck channel gain, and master gain commands using existing lock-free mixer queues.
- Return one combined snapshot for both decks and mixer health at 2 Hz.
- Keep one CoreAudio stream and one master output callback.
- Add no production dependency and make no database-schema change.
- Update the current placeholders with functional deck B and mixer controls without approving the final visual layout.

## Transition Safety

- Shut down the standalone deck-A service before the shared engine opens a stream.
- Do not run two application-owned output streams concurrently during migration.
- Preserve decoder-worker isolation, generation resets, stale-block rejection, resampling, and callback health counters.
- Keep database and UI operations outside the audio callback.

## Acceptance Checks

- Either deck can load first without requiring media on the other deck.
- Both decks play simultaneously through one shared callback.
- Independent transport controls, channel gains, crossfader, and master gain work from React.
- Existing deck-A command behavior remains compatible or is clearly migrated.
- Mixed-rate fixtures play together with accurate per-deck clocks.
- Missing/corrupt files and unavailable devices produce visible errors.
- Automated tests, integrated Tauri build, and Apple M3 two-deck hardware smoke test pass with healthy callback counters.

Approval authorizes this shared two-deck and basic mixer vertical slice. It does not approve final UI design, EQ, filters, loops, hot cues, sync, time-stretching, cue routing, effects, or AutoMix.

## Implementation Result

Approved by the owner and implemented on 2026-06-14.

- `MixerEngine` now opens one shared output stream with independently unloaded deck pipelines.
- The first track loaded on either deck starts only that deck's decoder worker; the second deck joins the existing callback.
- Tauri owns the engine on one mixer-service thread and returns a combined snapshot for both decks and mixer health.
- React provides independent Deck A and Deck B transport controls, two library load targets, channel gains, crossfader, and master gain.
- No dependency or SQLite schema changes were required.
- The previous Deck A commands now return the combined mixer snapshot; this is an intentional internal contract migration.
