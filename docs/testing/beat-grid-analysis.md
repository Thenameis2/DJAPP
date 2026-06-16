# Beat-Grid Analysis

## Implemented Scope

- Dynamic-programming links between spectral-flux onset peaks near the selected tempo.
- Weighted beat-line fitting and full-track grid extrapolation at the measured period.
- Per-beat onset strength and a separate grid confidence value.
- Conservative four-beat accent scoring for optional downbeat markers.
- Integer rational conversion from 22,050 Hz analysis positions to original source frames.
- Versioned beat-grid cache writing and path persistence through the existing analysis worker.

Low-evidence tracks retain BPM without publishing a beat-grid cache. Downbeats are omitted unless one four-beat phase clearly exceeds the alternatives. Sync, beat-aware loops, and AutoMix still do not consume the grid.

## Timing Benchmark

Run:

```sh
cargo run --offline --bin beat_grid_benchmark
```

Current deterministic results:

| Fixture | BPM | Beats | Median error | P95 error | Grid confidence | Downbeats |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Click | 60 | 24 | 9.524 ms | 10.476 ms | 0.963 | 0 |
| Click | 90 | 36 | 11.882 ms | 12.562 ms | 0.967 | 0 |
| Click | 120 | 48 | 10.612 ms | 10.703 ms | 0.865 | 0 |
| Click | 128 | 51 | 10.675 ms | 11.239 ms | 0.887 | 0 |
| Click | 150 | 60 | 11.429 ms | 11.701 ms | 0.880 | 0 |
| Click | 180 | 72 | 11.293 ms | 11.338 ms | 0.867 | 0 |
| Accented | 120 | 48 | 10.612 ms | 10.703 ms | 0.759 | 12 |
| Syncopated | 120 | 48 | 10.612 ms | 10.703 ms | 0.863 | 0 |

All deterministic fixtures meet ADR-012's maximum `20 ms` median absolute error target. The approximately 10 ms systematic offset is within one half-hop of the 512-frame onset resolution.

## Cache And Timeline

Beat positions are stored as original source-track frame indexes, not analysis-rate positions or seconds. Conversion multiplies by the source sample rate and divides by 22,050 using integer arithmetic with nearest-frame rounding. This avoids cumulative floating-point drift on long tracks.

Beat-grid format version 1 stores source sample rate, channels, source frame count, BPM, confidence, and ordered beat records. Each record contains a source frame, normalized onset strength, and downbeat flag. The cache is written to a temporary sibling, flushed, and atomically renamed before SQLite receives its path.

## Limitations

- The current benchmark covers steady-tempo synthetic material and the original music-like fixture, not a labeled private music corpus.
- The fitted grid can follow onset timing evidence but does not yet model deliberate tempo ramps or highly expressive live drumming.
- Meter estimation assumes a possible four-beat cycle and deliberately emits no downbeats when accent evidence is ambiguous.
- Grid corrections, Tauri cache queries, React display, and Sync consumption remain later approved stages.
