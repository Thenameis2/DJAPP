# BPM Analysis

## Implemented Scope

- Stereo or multichannel downmix to mono.
- Deterministic conversion to a 22,050 Hz analysis signal.
- Streaming 2,048-sample Hann-window FFT frames with 512-sample hops.
- Positive spectral-flux onset envelope with adaptive local-mean subtraction and normalization.
- Normalized autocorrelation candidates from 60 through 200 BPM.
- Harmonic-neighborhood scoring for half/double-tempo ambiguity.
- Sub-hop candidate interpolation and long-span onset-peak regression for precision.
- BPM confidence and up to eight diagnostic candidates.
- BPM and confidence persistence through the existing analysis worker.

The fixed-rate converter and FFT analysis are chunk-independent. Production processing retains only converter output for the current decoder chunk, FFT overlap, and the compact onset envelope; it does not retain full-track PCM.

Beat positions and conservative downbeat detection are implemented by stage five and documented in `docs/testing/beat-grid-analysis.md`. Musical key, Tauri/UI exposure, Sync, and AutoMix consumption remain unimplemented.

## Synthetic Benchmark

Run:

```sh
cargo run --offline --bin bpm_benchmark
```

Current deterministic results:

| Expected BPM | Measured BPM | Error |
| ---: | ---: | ---: |
| 60 | 60.005787 | 0.009645% |
| 90 | 89.993895 | 0.006783% |
| 120 | 119.998802 | 0.000998% |
| 128 | 127.993417 | 0.005143% |
| 150 | 149.996175 | 0.002550% |
| 180 | 179.999192 | 0.000449% |

The worst synthetic steady-tempo error is below the ADR-012 target of 0.1%.

The original 20-second music-like fixture measures 119.811985 BPM against its intended 120 BPM pulse, with confidence above the provisional 0.65 threshold. Accented and syncopated 120 BPM fixtures verify that the scorer does not select 60 or 180/240 BPM alternatives. Silence and the steady-tone fixtures return no BPM.

To inspect an individual supported file without persisting data:

```sh
cargo run --offline --bin bpm_benchmark -- /path/to/track.mp3
```

This prints the selected estimate and ranked candidates. Private music paths and results must not be committed.

## Verification

Tests cover fixed-rate conversion, chunk-boundary equivalence, bounded streaming FFT state, onset finiteness, silence rejection, steady-tone rejection, six steady tempos, accented beats, syncopation, music-like audio, result persistence, and all supported decoder formats.

Current result after stage five: 76 engine tests, eight CLI tests, and two default Tauri tests pass. Two direct CoreAudio tests remain ignored unless the required hardware outputs are available.
