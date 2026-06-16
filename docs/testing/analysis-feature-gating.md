# Analysis Feature Gating

## Stage 9 Decision

Stage 9 accepts the analysis pipeline for library metadata, cache reuse, queue visibility, manual correction workflows, and developer diagnostics.

Stage 9 does not accept generated BPM or beat-grid data for live Sync, beat-aware AutoMix, compatibility-based AutoMix reordering, or downbeat-dependent transitions.

## Evidence Accepted

- Synthetic BPM benchmark worst error remains below the ADR target.
- Synthetic beat-grid median timing errors remain below the ADR target.
- Synthetic key fixtures classify correctly.
- Cache reopen works without decoding the source file again.
- Active cancellation does not publish partial waveform or beat-grid artifacts.
- Apple M3 playback-under-analysis completed without stream errors, deck underflows, recycle failures, or worker errors in the documented muted mixed-rate scenario.
- The UI exposes analysis status, queue order, cancellation, uncertainty indicators, and manual corrections.

## Evidence Missing

- No labeled real-music BPM corpus has been accepted.
- No representative full-library soak has been accepted.
- No confidence calibration has been accepted for real music.
- No real-music beat-grid timing report has been accepted.
- Owner feedback reports inaccurate BPM compared with other DJ software.

## Current Feature Gates

| Feature | Stage 9 Status | Reason |
| --- | --- | --- |
| Library BPM display | Allowed with uncertainty marker | Useful metadata, user can correct it |
| Library key display | Allowed with uncertainty marker | Useful metadata, user can correct it |
| Waveform cache/display | Allowed | Cache validity and publication are covered |
| Beat-grid cache | Allowed as metadata | Needed for future work, not trusted for live sync |
| Manual BPM/key correction | Allowed | User-authored data overrides generated analysis |
| Master/follower Sync | Blocked | Real-music BPM/grid accuracy is unaccepted |
| AutoMix transition timing | Blocked | Depends on trusted BPM/grid timing |
| AutoMix compatibility reordering | Blocked | Depends on trusted BPM/key confidence calibration |
| Downbeat-dependent transitions | Blocked | Downbeat confidence is unaccepted on real music |
| Automatic BPM/key correction | Blocked | Would hide algorithm uncertainty |

## Next Required Acceptance Work

Create a private local benchmark manifest outside the repository, or under ignored `private-benchmarks/`. The manifest workflow is documented in `docs/testing/bpm-accuracy-milestone.md`.

Each entry should include:

- track path;
- expected BPM from the owner's comparison software;
- whether half-time or double-time alternatives are acceptable;
- optional notes about genre, intro/outro complexity, live drums, tempo drift, or known failure mode.

Run the benchmark against easy, hard, and currently failing tracks. Then tune BPM ambiguity handling and confidence thresholds so unreliable results are visibly uncertain or rejected before any live feature consumes them.

If the project-owned algorithm cannot meet the ADR target on the accepted corpus, prepare a new dependency/licensing ADR instead of enabling Sync with weak evidence.
