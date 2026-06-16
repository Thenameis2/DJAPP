# Musical-Key Analysis

## Implemented Scope

- Streaming 8,192-sample Hann-window FFT analysis at 22,050 Hz with 4,096-sample hops.
- Local spectral-peak extraction from 55 Hz through 5 kHz.
- Pitch-class chroma accumulation with estimated tuning offset in cents.
- Per-frame normalization, low-energy rejection, harmonic-support gating, and broadband-uniformity rejection.
- Correlation against rotated major and minor tonal profiles for all 24 canonical keys.
- Independent key confidence based on candidate separation, tonal concentration, and accepted-frame coverage.
- Canonical persistence as `pitch-class:major|minor`; Camelot notation remains presentation-only.

Silence, broadband noise, and the pitched-percussion click fixture do not claim a key. Low-confidence estimates remain generated metadata and must be displayed as uncertain below the provisional `0.60` UI threshold.

## Synthetic Benchmark

Run:

```sh
cargo run --offline --bin key_benchmark
```

The benchmark evaluates all 12 roots in both major and minor modes and reports each result as exact, relative/parallel, neighboring Camelot, or incorrect. Current result:

| Classification | Count |
| --- | ---: |
| Exact | 24 |
| Relative or parallel | 0 |
| Neighboring Camelot | 0 |
| Incorrect or unavailable | 0 |

Additional tests correctly identify A minor at tuning offsets of `-35`, `-20`, `+20`, and `+35` cents. Reported tuning estimates are diagnostic metadata and are not currently persisted.

## Correction Precedence

Generated BPM and key remain in `track_analysis`. User-authored BPM, key, and beat-grid offsets remain separately stored in `track_corrections`.

`effective_analysis(track_id)` applies these rules:

1. A valid user correction overrides the corresponding generated BPM or key.
2. Confidence is unavailable for a corrected value because generated confidence does not describe the user's choice.
3. Uncorrected fields continue to use generated values and confidence.
4. The generated record remains available unchanged for re-analysis, diagnostics, and reverting a correction.

Correction writes accept only positive finite BPM and canonical `0..11:major|minor` key strings. No schema migration was introduced.

## Limitations

- The benchmark uses clean synthetic triads. A labeled music corpus is still required before key-driven AutoMix ranking is accepted.
- Modulating, ambiguous, atonal, and heavily percussive tracks may return a low-confidence key or no key.
- The estimator aggregates full-track tonal evidence and does not expose section-by-section key changes.
- Tauri commands, React display, and correction controls remain stage-seven work.
