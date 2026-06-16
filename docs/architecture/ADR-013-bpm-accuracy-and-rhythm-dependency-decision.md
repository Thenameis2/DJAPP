# ADR-013: BPM Accuracy And Rhythm Dependency Decision

- Status: Accepted; in-house deep beat-tracking spike stage five implemented
- Date: 2026-06-16
- Scope: BPM accuracy, beat tracking, dependency/licensing decision, and Sync/AutoMix gating

## Context

ADR-012 approved a small, permissive, project-owned analysis pipeline using `rustfft` and `ebur128`. Stages one through nine are implemented, and generated analysis is available for library metadata, waveform/loudness/key display, manual corrections, queue visibility, cache reuse, and diagnostics.

Owner testing against a five-track private manifest showed that generated BPM is still not reliable enough for Sync or AutoMix:

- initial real-track benchmark: `0/5` exact BPMs accepted;
- after confidence capping, segment consensus, beat-support ranking, low/low-mid weighting, neutral octave ambiguity, and multi-band rhythm evidence: `1/5` exact BPMs accepted;
- wrong and near-correct candidates often receive very similar grid and band-consensus evidence;
- generated BPM confidence is now conservative, but the selected BPM is still often inaccurate.

This ADR evaluates whether to continue deeper project-owned rhythm work or introduce a dedicated rhythm-analysis dependency.

Owner approval on 2026-06-16 selected the recommended path: implement the in-house deep beat-tracking spike first, without adding a production dependency.

## Decision Drivers

1. DJ-grade BPM and beat-grid accuracy on owner-selected real music.
2. Compatibility with future public distribution.
3. Offline-first macOS Apple-silicon desktop app.
4. No audio callback risk; all analysis stays in the background worker.
5. Reasonable native build surface for Tauri.
6. Avoiding copyleft obligations.
7. Testability against the private manifest and redistributable fixtures.

## Current Project-Owned Path

The current Rust implementation already includes:

- mono fixed-rate analysis signal at 22,050 Hz;
- spectral-flux onset extraction;
- weighted low/low-mid flux;
- full-track autocorrelation candidates;
- segment consensus;
- half/double variants;
- beat-grid support diagnostics;
- cross-band rhythm envelopes;
- confidence capping for close or octave-ambiguous candidates.

This path is dependency-light and license-safe. Its weakness is algorithmic: the current onset evidence is not discriminating enough on real commercial tracks, especially when vocals, syncopation, half-time feel, sparse drums, or strong subdivisions compete with the reference BPM.

## Options Considered

### Option A: Continue Project-Owned Deep Beat Tracking

Implement a deeper in-house rhythm tracker:

- finer frequency-band onset functions, including separate kick, snare/clap, hi-hat, and broadband transient evidence;
- tempo state tracking over time rather than whole-track candidate ranking only;
- dynamic-programming or HMM-style beat sequence scoring over candidate tempo curves;
- downbeat/phrase support as a separate weak signal;
- explicit octave ambiguity output, not silent correction;
- stronger benchmark diagnostics for where each candidate wins or loses.

**Apple-silicon support:** strong, pure Rust plus existing dependencies.

**Maintenance:** owned by the project; no native dependency churn, but algorithm work can take time.

**Licensing:** best fit; current dependencies are permissive.

**Performance:** predictable background-worker cost; can be optimized incrementally.

**Maturity:** currently below DJ-grade on the private benchmark.

**Tauri compatibility:** excellent because the code is already in the Rust engine.

**Tradeoff:** safest architecture, but accuracy risk remains with no guarantee of reaching professional-app behavior quickly.

### Option B: Add aubio Through Rust Bindings Or A Local Spike

aubio is a C music-analysis library with onset detection, tempo tracking, and beat detection. Its README lists tempo tracking and beat detection, and the project includes tools such as `aubiotrack` for beat timestamps. It compiles on macOS according to the upstream README. Sources checked:

- https://github.com/aubio/aubio
- https://raw.githubusercontent.com/aubio/aubio/master/README.md
- https://raw.githubusercontent.com/aubio/aubio/master/COPYING
- https://github.com/katyo/aubio-rs

**Apple-silicon support:** plausible but must be proven. Upstream documents macOS support, and `aubio-rs` exposes build features including built-in C compilation, `pkg-config`, static/shared linking, and Apple Accelerate support.

**Maintenance:** aubio itself is mature but its latest GitHub release shown during review was `0.4.9` from 2019. `aubio-rs` appears small and older, with a latest release shown from 2020. That increases integration risk.

**Licensing:** poor fit for possible public distribution. aubio is GPL-3.0-or-later. Linking it into the app could impose GPL obligations on distributed builds. This may be acceptable for private-only use, but it conflicts with the current “may become public” requirement unless the owner accepts GPL terms.

**Performance:** likely suitable for offline analysis; must be benchmarked on Apple M3.

**Maturity:** better established than the project-owned implementation for basic onset/tempo/beat tasks, but not guaranteed to match commercial DJ apps.

**Tauri compatibility:** possible via Rust FFI/build script, but adds native C build and packaging complexity.

**Tradeoff:** good candidate for a private local spike, not a production dependency unless GPL obligations are accepted.

### Option C: Add Essentia

Essentia is a C++ music-information-retrieval library. Its documentation includes `RhythmExtractor2013`, which outputs BPM, beat tick locations, confidence, BPM estimates, and beat intervals, with `multifeature` and `degara` beat-tracking methods. Sources checked:

- https://github.com/MTG/essentia
- https://essentia.upf.edu/reference/std_RhythmExtractor2013.html
- https://essentia.upf.edu/algorithms_reference.html

**Apple-silicon support:** likely possible but heavier than aubio. Upstream lists macOS support, but production integration would require C++ build and packaging validation on Apple silicon.

**Maintenance:** mature and broad MIR library, but the official GitHub latest release shown during review was old (`2.0.1` from 2014), while active development appears to continue on master/docs. This needs a spike before relying on it.

**Licensing:** poor fit for future public distribution. Upstream describes Essentia as AGPLv3. AGPL is stronger copyleft than GPL for this application’s future public-distribution goals.

**Performance:** likely acceptable offline, but heavier than needed if the app only needs BPM/beat-grid validation.

**Maturity:** strong MIR capability and richer rhythm algorithms than our current implementation.

**Tauri compatibility:** possible but high integration cost: C++ library, build tooling, binary size, and packaging complexity.

**Tradeoff:** technically attractive as a benchmark oracle, but licensing and integration cost make it a poor production dependency for this app.

### Option D: Use madmom As An External Reference Only

madmom is a Python MIR library with strong beat/downbeat research algorithms and command-line beat trackers. Sources checked:

- https://github.com/CPJKU/madmom
- https://raw.githubusercontent.com/CPJKU/madmom/master/LICENSE

**Apple-silicon support:** uncertain for app bundling because it depends on Python, NumPy/SciPy/Cython, model/data files, and sometimes FFmpeg.

**Maintenance:** mature research project, but it is not a natural Rust/Tauri production dependency.

**Licensing:** source code is BSD, but model/data files are Creative Commons Attribution-NonCommercial-ShareAlike 4.0 unless otherwise indicated. That is not suitable for a future commercial/public app if model files are required.

**Performance:** potentially strong but not appropriate inside the desktop app.

**Maturity:** strong as a research reference.

**Tauri compatibility:** poor for production; acceptable as an optional external comparison tool outside the app.

**Tradeoff:** useful for offline benchmarking and algorithm inspiration, not production integration.

## Recommendation

Do not add a production rhythm dependency yet.

Implement a bounded **rhythm accuracy spike** with two tracks:

1. **In-house deep beat-tracking spike**
   - Keep the current dependency set.
   - Implement a tempo-state/beat-sequence scorer over the existing fixed-rate analysis signal.
   - Add diagnostics that show candidate evidence over time, not only aggregate scores.
   - Target the private five-track manifest first.

2. **Non-production aubio comparison spike**
   - Build aubio only in a throwaway local spike or feature-gated diagnostic binary.
   - Do not ship it in the Tauri app.
   - Compare aubio BPM/beat timestamps against the same private manifest.
   - Use the result to decide whether an external algorithm family materially improves the failing tracks.

Reject Essentia and madmom as production dependencies for now because their licensing and packaging costs are too high for this project’s current future-public-distribution requirement.

## Implementation: In-House Spike Stage One

Implemented on 2026-06-16:

- added temporal tempo-state scoring over overlapping 12-second windows;
- each BPM candidate now records `state`, showing whether local tempo candidates and grid evidence support that BPM across time;
- `bpm_benchmark` prints `state` in candidate diagnostics;
- `ANALYSIS_VERSION` is now `7`.

Private five-track benchmark result remains `1/5` exact BPMs accepted. The spike improved diagnostics and moved one failure away from a half-tempo interpretation, but it did not meet the acceptance gate. No production dependency, schema, or UI change was made.

## Proposed Acceptance Gates

Before generated BPM can drive Sync or AutoMix:

- private BPM benchmark reaches at least `90%` accepted tracks at `<=1%` error, including explicitly allowed half/double equivalents;
- no high-confidence wrong BPM rows remain in the representative private manifest;
- benchmark includes at least 25 owner-approved real tracks across genres and formats;
- accepted beat grids have median beat timing error below `20 ms` where a reference grid is available;
- playback-under-analysis hardware test still passes on the Apple M3;
- manual BPM correction always overrides generated BPM.

Before any copyleft dependency can be added to production:

- owner explicitly approves the license implications;
- distribution target is clarified as private-only or GPL/AGPL-compatible;
- macOS Apple-silicon packaging is proven;
- binary size and build reproducibility are documented;
- Sync/AutoMix remains gated until the private benchmark passes.

## Consequences

- Sync and analysis-driven AutoMix remain disabled.
- The current analysis pipeline remains valid for metadata display, manual corrections, waveform/loudness/key, and diagnostics.
- The next implementation should be a spike, not a permanent dependency addition.
- If the in-house spike does not substantially improve the private manifest, the project should either accept manual BPM corrections for version one or make a deliberate licensing decision for an external rhythm engine.

## Remaining Decision Needed From Owner

Approve one of these next actions before further BPM architecture work:

1. **Continue in-house:** implement a deeper beat-sequence/tempo-curve tracker.
2. **Compare aubio:** run the non-production aubio comparison spike, accepting that it is GPL and not approved for shipping.
3. **Manual-first:** stop BPM algorithm work for now and make manual BPM correction/editing smoother for version one.

## Implementation: Non-Production aubio Comparison Harness

Implemented on 2026-06-16:

- added `src/bin/aubio_compare.rs`;
- the binary shells out to an installed `aubiotrack` command and estimates BPM from returned beat timestamps;
- no aubio Rust crate, C library, build script, linker flag, or Tauri integration was added;
- the harness uses the same private TSV manifest format and allowed half/double scoring as `bpm_benchmark`;
- output is CSV for side-by-side comparison with the project-owned analyzer.

Run after installing aubio locally:

```sh
cargo run --offline --bin aubio_compare -- --manifest ~/Djapp-BPM-Test/private-bpm.local.tsv
```

If `aubiotrack` is not on `PATH`, pass it explicitly:

```sh
cargo run --offline --bin aubio_compare -- --manifest ~/Djapp-BPM-Test/private-bpm.local.tsv --aubiotrack /path/to/aubiotrack
```

This is diagnostic-only. It must not be wired into production analysis or bundled with the app without a separate owner approval of GPL license implications.

After installing aubio `0.4.9` through Homebrew, the default `aubiotrack` comparison accepted `0/5` exact BPMs on the private manifest. Three rows were within roughly `1.5-2.2%`, but none met the `<=1%` gate, and two rows were far from the reference BPM. This result does not justify a production aubio dependency. Further aubio testing, if any, should remain diagnostic and explore alternate onset methods or thresholds before changing this conclusion.

## Implementation: In-House Spike Stage Two

Implemented on 2026-06-16:

- added scratch-built comb-filter/tempogram-style candidate generation over the onset envelope;
- each BPM candidate now records `comb`, showing repeated-beat comb-filter support;
- `bpm_benchmark` prints `comb` in candidate diagnostics;
- `ANALYSIS_VERSION` is now `8`.

Private five-track benchmark result remains `1/5` exact BPMs accepted. The comb-filter signal is permissively implemented and useful as another diagnostic, but it did not separate the current failure set because `comb` values are low and similar across competing candidates. This result reinforces that the next in-house step needs deeper beat-sequence or tempo-curve tracking, not more simple aggregate weighting.

## Implementation: In-House Spike Stage Three

Implemented on 2026-06-16:

- added scratch-built dynamic-programming beat-sequence support over onset peaks;
- each BPM candidate now records `seq`, showing whether that BPM can form a coherent beat chain through the track;
- `bpm_benchmark` prints `seq` in candidate diagnostics;
- `ANALYSIS_VERSION` is now `9`.

Private five-track benchmark result remains `1/5` exact BPMs accepted. The sequence signal is useful for explaining why half/double candidates can appear convincing, but it did not improve the acceptance set. On `ffawty - Stay Home`, the sequence score favored the `77.005200` half-tempo chain again, while the nearest candidate to expected `151.0` remains `153.992705`. Further in-house work should separate tempo-octave selection from beat-chain existence rather than adding more aggregate weights.

## Implementation: In-House Spike Stage Four

Implemented on 2026-06-16:

- added an explicit tempo-octave resolver after candidate scoring;
- each BPM candidate now records `oct`, showing the resolver's preference after combining score, temporal support, section consistency, band consensus, and half/double relationships;
- close half/double families now favor the candidate with stronger time-consistent support instead of letting the coherent beat-chain score alone decide;
- `bpm_benchmark` prints `oct` in candidate diagnostics;
- `ANALYSIS_VERSION` is now `10`.

Private five-track benchmark result remains `1/5` exact BPMs accepted. The resolver improved the main half-tempo failure shape: `ffawty - Stay Home` now selects `153.992705` instead of `77.005200`. `U Fancy` and `LUCKI` also keep their double-tempo interpretations over coherent half-tempo alternatives. However, the remaining errors are still outside the `<=1%` gate, and `Lil Gnar` still ranks the wrong tempo family first. This stage improves octave selection but does not yet solve candidate precision or hard syncopated-trap ranking.

## Implementation: In-House Spike Stage Five

Implemented on 2026-06-16:

- added bounded local BPM precision refinement around each candidate using beat-grid evidence;
- each BPM candidate now records `fit`, the percent adjustment applied by the precision pass;
- added a narrow near-tie breaker that prefers a candidate with stronger grid, beat-strength, and temporal evidence when the current winner is only marginally ahead;
- `bpm_benchmark` prints `fit` in candidate diagnostics;
- `ANALYSIS_VERSION` is now `11`.

Private five-track benchmark improved from `1/5` to `4/5` exact BPMs accepted:

- `ffawty - Stay Home`: `151.297833` for expected `151.0`, accepted at `0.197240%`;
- `Lil Tecca - Down With Me`: `115.140096` for expected `115.0`, accepted at `0.121823%`;
- `U Fancy`: `148.193898` for expected `148.0`, accepted at `0.131012%`;
- `LUCKI - MORE THAN EVER`: `151.991445` for expected `152.0`, accepted at `0.005628%`;
- `Lil Gnar - Welcome 2 Da Game` still fails because the correct-neighborhood `75.417621` candidate is present and within the `<=1%` gate for expected `76.0`, but the analyzer still ranks `94.233097` first.

This stage materially improves candidate precision and validates that the in-house path can get close on most of the initial private manifest. Generated BPM still must not drive Sync or AutoMix because the current acceptance is `80%`, below the ADR gate of `90%`, and the benchmark is still only five tracks rather than the required representative set.
