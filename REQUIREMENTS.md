# DJ Application Requirements

## 1. Product Summary

Build a private-use, offline-first macOS DJ application for an Apple M3 computer. The application must allow a DJ to perform from local music files without an external DJ controller. It may support controllers later and may eventually be prepared for public distribution.

The experience should be inspired by the workflow and polish of professional two-deck DJ software, including djay Pro, without copying proprietary branding, artwork, or source code.

## 2. Product Goals

- Provide a capable two-deck DJ workflow using only the computer.
- Play and mix music stored in local folders without Wi-Fi.
- Provide automatic track analysis and cache the results locally.
- Support manual mixing and an intelligent AutoMix mode.
- Work with built-in speakers, wired devices, Bluetooth output, and compatible external audio hardware exposed by macOS.
- Remain maintainable, private, and free of subscriptions or locked features.

## 3. Non-Goals For Version 1

- User accounts or roles.
- Cloud synchronization or a hosted backend.
- Music-streaming integrations.
- Payments, subscriptions, bookings, messaging, song requests, or live social features.
- Recording completed DJ mixes.
- Native mobile applications.
- Windows or Linux support.
- Guaranteed support for every MIDI controller.

## 4. Target Platform

- macOS on Apple silicon, initially tested on an Apple M3 computer.
- Installable desktop application.
- Fully usable without an internet connection after installation.
- A web-only deployment on Vercel or Railway is not a version-one target because browser audio and local-file restrictions conflict with the required workflow.

## 5. Functional Requirements

### 5.1 Local Music Library

- The user can select one or more local folders.
- Folder scanning is recursive and includes nested genre or organization folders.
- The app displays discovered tracks without moving or altering source files.
- Initial formats: MP3, WAV, FLAC, AAC/M4A, and AIFF, subject to decoder and codec availability.
- The library shows useful metadata including title, artist, album, duration, format, BPM, key, and analysis status when available.
- The app handles missing, moved, renamed, duplicate, corrupt, and unsupported files gracefully.
- The user can rescan folders and remove a folder from the app's library without deleting music from disk.

### 5.2 Track Analysis

The application analyzes and locally caches:

- BPM.
- Beat grid and downbeats where technically practical.
- Musical key.
- Waveform overview and detailed waveform data.
- Peak level and perceived loudness suitable for gain normalization.

Analysis must run outside the real-time audio path, show progress, be cancellable where practical, and reuse valid cached results when tracks are reopened.

### 5.3 Two Virtual Decks

- Two independent decks labeled A and B.
- Load a track from the library or queue into either deck.
- Play, pause, cue, seek, and return to cue.
- Sync and master-deck selection.
- Tempo adjustment, pitch bend, and jog-wheel interaction.
- Per-deck gain.
- Three-band EQ: high, mid, and low.
- Filter control.
- Configurable loops.
- Multiple hot cues stored locally per track.
- Effects controls, with the exact version-one effect set chosen during design approval.
- Clear waveform, playhead, elapsed/remaining time, BPM, key, tempo, and deck-state displays.

### 5.4 Mixer

- Crossfader between decks A and B.
- Per-deck channel volume.
- Master gain and metering.
- Per-deck level meters and clipping indication.
- Headphone cue selection for deck A, deck B, or both.
- Cue/master blend control.
- Controls must be accessible with pointer or trackpad; keyboard shortcuts are optional.

### 5.5 Audio Output And Cueing

- The user can select an output device exposed by macOS.
- Built-in computer speakers are supported as master output.
- Wired speakers, headphones, audio interfaces, and compatible external devices exposed by macOS should be selectable.
- Bluetooth speakers may be used for master output, with a visible warning that Bluetooth latency can make live beat mixing inaccurate.
- Output-device disconnection must not crash the application.
- The app must clearly communicate unavailable routing combinations.

Important constraint: independent stereo master output and private stereo headphone cue normally require multiple addressable output channels, an external audio interface/controller, a configured macOS aggregate or multi-output device, or a lower-quality split-output mode. With only the built-in audio output, plugging in headphones may replace the speaker output rather than create an independent cue channel. Version 1 must detect capabilities and avoid promising impossible routing.

### 5.6 Queue And AutoMix

- The user can create a temporary play queue from scanned local tracks.
- AutoMix automatically selects transition timing, beat-matches tracks, and performs transitions.
- When AutoMix is enabled, the user chooses one of two modes:
  - Preserve the current queue order.
  - Reorder upcoming tracks using BPM, key, energy, duration, or other compatibility signals.
- AutoMix must not overwrite or modify source audio files.
- The interface must show the current track, next track, selected mode, and enough transition state for the user to take manual control.
- The user can stop AutoMix and resume manual control without interrupting playback unnecessarily.

### 5.7 Persistence

The application stores data locally, with SQLite proposed for structured data. Persisted data includes:

- Application settings and preferences.
- Music-folder locations and library metadata.
- Cached track-analysis results and versions.
- Hot cues, loops, beat-grid corrections, and per-track metadata corrections.
- Queue state when appropriate.
- Audio-device and routing preferences.
- Future controller mappings.

No online database is required. The application should provide a documented way to locate, back up, and reset its local data.

### 5.8 External Controller Support

External hardware is not required to use the application. The architecture should not prevent future MIDI controller support.

Generic MIDI mapping and specific controller support are deferred until requirements and test hardware are available. They are not release blockers for version 1.

## 6. User Experience Requirements

- Professional two-deck interface inspired by modern DJ software, with dark theme as the default.
- Provide both light and dark themes, with the user's selection saved locally.
- Layout optimized for a macOS laptop screen and trackpad use.
- Large, discoverable controls for live use in dim environments.
- Clear loading, analysis, clipping, latency, missing-file, and device-error states.
- Smooth visual feedback without compromising audio-thread performance.
- Do not reproduce djay Pro trademarks or protected visual assets.

## 7. Non-Functional Requirements

### 7.1 Performance And Reliability

- Audio playback must remain stable during library scans and track analysis.
- Real-time audio processing must not run on the UI thread.
- The app should start and browse already indexed music while offline.
- Track loading and cached waveform display should feel immediate on supported hardware.
- Exact latency, CPU, memory, startup, and library-size targets must be benchmarked and approved during audio-engine design.

### 7.2 Privacy And Security

- Music and analysis data stay on the user's computer.
- No account, telemetry, analytics, or network access by default.
- Request only the macOS file permissions needed for user-selected folders.
- Never edit or delete original music files.
- Do not store secrets or unnecessary personal data.

### 7.3 Accessibility

- Controls must have readable labels and sufficient contrast.
- Do not rely on color alone to communicate deck, cue, clipping, or playback state.
- Provide visible focus states and semantic accessibility labels where supported.
- Core controls should remain operable through standard macOS accessibility APIs where practical.

### 7.4 Maintainability

- Use clear module boundaries between UI, library, analysis, audio engine, persistence, and device routing.
- Test changed behavior and update documentation after every meaningful change.
- Use versioned database migrations and versioned analysis-cache formats.
- Keep third-party dependencies limited and documented.

## 8. Proposed Technology Direction

- Tauri for the installable desktop shell.
- React with TypeScript for the interface.
- Rust for audio decoding, analysis, real-time mixing, and macOS device interaction.
- SQLite for local persistence.

This architecture was approved by the owner on 2026-06-13. The developer must present an architecture note covering candidate audio, decoding, time-stretching, key-detection, and waveform libraries before adding production dependencies. Licensing must be reviewed with possible future public distribution in mind.

## 9. Version 1 Acceptance Criteria

Version 1 is finished when, on the target Apple M3 Mac:

- The app installs, launches, and performs its core workflow without Wi-Fi.
- The user can add a folder tree and browse supported local tracks.
- Analysis produces cached BPM, beat-grid, key, waveform, and loudness data with documented limitations.
- Two decks can load and play tracks independently.
- Play/pause, cue, sync, tempo, pitch bend, jog, gain, EQ, filter, loops, hot cues, effects, channel volume, and crossfader work reliably.
- The user can select available master and cue outputs, and unsupported routing is clearly explained.
- Built-in and wired master output work; Bluetooth output works when macOS exposes it, with a latency warning.
- AutoMix works in preserve-order and compatibility-reorder modes.
- Settings, library information, analysis, and cue data survive application restarts.
- Light and dark themes work throughout the interface, and the selected theme survives application restarts.
- Device loss, missing files, unsupported files, and corrupt files do not crash the app.
- Automated tests pass, the macOS build succeeds, and a documented manual DJ workflow test passes.
- Documentation and `MEMORY.md` accurately reflect the released behavior and known limitations.

## 10. Open Decisions

- Select and approve the exact initial effects.
- Select and approve the Rust audio, decoding, analysis, and time-stretching dependencies.
- Define measurable audio latency, stability, analysis-accuracy, and library-scale targets.
- Decide whether to include a split-output cue mode for users without multi-channel hardware.
- Decide whether macOS aggregate-device setup should be documented or assisted in-app.
- Decide whether generic MIDI mapping belongs in a later version.
- Decide how the private application will be packaged and signed.
