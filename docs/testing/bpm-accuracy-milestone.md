# BPM Accuracy Milestone

## Goal

Tune and accept BPM analysis against representative real music before generated BPM or beat grids are allowed to drive Sync or AutoMix.

This milestone uses private local music and expected BPM values from the owner's comparison DJ software. Do not commit copyrighted audio, private paths, or benchmark output that reveals private library details.

## Private Manifest Format

Create a local tab-separated manifest outside the repository, or under ignored `private-benchmarks/`.

Each non-comment line uses:

```text
expected_bpm<TAB>path<TAB>allowed<TAB>notes
```

- `expected_bpm`: BPM from the comparison software.
- `path`: absolute path, or path relative to the manifest file.
- `allowed`: optional. Use `exact`, `half`, `double`, `half-or-double`, or `any`.
- `notes`: optional context such as genre, intro complexity, live drums, or known failure mode.

Example:

```text
# expected_bpm	path	allowed	notes
128.0	/Users/me/Music/house-track.mp3	exact	clean electronic beat
75.0	/Users/me/Music/trap-song.m4a	any	other software may show 75 or 150
143.0	../Music/drill.wav	half-or-double	syncopated intro
```

## Run

```sh
cargo run --offline --bin bpm_benchmark -- --manifest /path/to/private-bpm.local.tsv
```

The command prints one CSV row per track:

- label;
- expected BPM;
- measured BPM or `unavailable`;
- best error percentage after applying the allowed half/double rule;
- confidence;
- accepted flag;
- nearest candidate BPM;
- nearest candidate error percentage;
- top candidates as `bpm@score`;
- allowed rule;
- notes.

For failed rows, the candidate columns tell us whether the correct BPM is present but ranked too low, or whether the analyzer never found the right tempo candidate.

The command exits successfully only when at least 90% of tracks are accepted with `<=1%` BPM error, including explicitly allowed half/double equivalents.

## Acceptance Gate

Generated BPM remains metadata-only until this milestone passes on an owner-approved representative set.

The benchmark set should include:

- clean electronic tracks expected to pass;
- songs that currently fail;
- slow tracks where half/double ambiguity is common;
- syncopated, intro-heavy, or live-drum tracks;
- MP3, WAV, FLAC, AAC/M4A, and AIFF if available in the owner's library.

## Next Engineering Loop

1. Run the private manifest.
2. Identify rejected rows and low-confidence accepted rows.
3. Tune tempo candidate scoring and confidence thresholds.
4. Re-run synthetic tests, private benchmark, and playback-under-analysis checks.
5. Update ADR-012 and memory with the new acceptance result.

If the project-owned algorithm cannot reach the ADR target, prepare a dependency/licensing ADR rather than enabling Sync with weak BPM evidence.

## First Private Benchmark Finding

The first owner-provided manifest contained five real tracks and accepted `exact` BPM only. The initial run accepted `0/5` tracks, and all wrong estimates reported high confidence around `0.78-0.82`.

The first safety tuning pass changed confidence calibration so close competing candidates cap confidence. The same five tracks still do not meet exact BPM accuracy, but their generated confidence now falls below the provisional `0.65` trust threshold. This means they should be displayed as uncertain after re-analysis rather than treated as reliable metadata.

This is a safety improvement, not final BPM acceptance. Candidate generation/ranking still needs further tuning before Sync or AutoMix can consume generated BPM.

## Segment Consensus Pass

The next tuning pass added section-level tempo consensus. The analyzer now supplements full-track autocorrelation candidates with repeated candidates found in overlapping 20-second onset-envelope windows, including half/double variants. This helps when the whole-track average is pulled away from the dominant musical pulse by intros, outros, sparse sections, or competing subdivisions.

On the first five-track private manifest, exact-BPM acceptance changed from `0/5` to `1/5`:

- `LUCKI - MORE THAN EVER` now measures `152.723282` against expected `152.0`, accepted at `0.475843%` error.
- `U Fancy` now includes a correct candidate at `148.969307`, but still ranks `170.318198` first.
- `ffawty - Stay Home` moved closer from `162.160711` to `154.770243`, but remains outside the `1%` target for expected `151.0`.
- `Lil Tecca - Down With Me` and `Lil Gnar - Welcome 2 Da Game` remain inaccurate.

This is an incremental accuracy improvement, not final acceptance. `ANALYSIS_VERSION` is now `3` so cached analysis created by earlier BPM ranking logic is considered stale.

## Beat-Support And Frequency-Weighted Pass

The next pass added three pieces that resemble the way DJ apps resolve candidate tempos:

- candidate reranking by regular beat-grid support against the onset envelope;
- extra spectral-flux weight for low and low-mid frequency changes, so kicks influence the onset envelope more than broadband vocal or high-hat movement;
- octave correction when a half/double candidate is nearly tied with the top candidate.

The five-track private manifest remains `1/5`, but the failure shape improved:

- `ffawty - Stay Home` no longer selects the `77 BPM` half-tempo interpretation; it now measures `153.992705` for expected `151.0`.
- `Lil Tecca - Down With Me` now measures `118.884973` for expected `115.0`.
- `U Fancy` now measures `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.
- `Lil Gnar - Welcome 2 Da Game` remains the hardest failure; nearest candidate is `77.870544` for expected `76.0`, but it does not rank first.

This is still not Sync or AutoMix acceptance. `ANALYSIS_VERSION` is now `4` so cached analysis created before beat-support/frequency-weighted ranking is considered stale.

## Candidate Grid-Scoring Diagnostics

The next implementation removed the previous tempo-range octave correction and replaced it with neutral half/double ambiguity handling. The analyzer now lets the strongest candidate win by evidence, and caps confidence when a half/double-related candidate is close enough to make the tempo octave uncertain. This avoids genre-specific rules such as forcing trap or hip-hop into a fixed BPM range.

Each candidate now carries full-track grid diagnostics:

- `grid`: combined beat-grid support score;
- `beat`: mean predicted-beat onset strength;
- `contrast`: predicted beat strength compared with halfway off-beats;
- `stable`: consistency of beat strengths;
- `sections`: support across song sections;
- `amb`: whether a close half/double candidate makes the tempo octave ambiguous.

The benchmark candidate column now prints those values, for example:

```text
153.993@0.712[grid=0.587,beat=0.107,contrast=0.608,stable=0.856,sections=0.993,bands=0.724,amb=y]
```

On the first five-track private manifest, exact-BPM acceptance remains `1/5`. The diagnostic result shows the current grid score is still not discriminating strongly enough: several wrong and near-correct candidates receive very similar grid support, so the ranking problem is now visible rather than solved.

- `ffawty - Stay Home` regressed to the `77.005200` half-tempo candidate by a small score margin, while `153.992705` remains the nearest candidate for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still ranks `94.897379` first; the nearest candidate to expected `76.0` is `77.870544`.
- `Lil Tecca - Down With Me` remains at `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

This phase is useful diagnostic infrastructure but not a quality acceptance. `ANALYSIS_VERSION` is now `5` so cached analysis created before candidate grid diagnostics and neutral octave ambiguity handling is considered stale.

## Multi-Band Rhythm Evidence Pass

The next implementation kept the same dependency set and split each FFT hop into separate low, low-mid, mid, and high rhythm envelopes. Candidate generation now includes cross-band consensus candidates, and candidate scoring records `bands`, a cross-band agreement score.

The five-track private manifest still remains `1/5`, which is an important finding: the current band evidence is too broad to resolve the hard tracks. Several wrong, half/double, and near-correct candidates all receive high band-consensus values around the same range, so the analyzer still cannot reliably choose the reference BPM from these signals alone.

- `ffawty - Stay Home` still picks `77.005200`, with nearest candidate `153.992705` for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still picks `94.897379`, with nearest candidate `77.870544` for expected `76.0`.
- `Lil Tecca - Down With Me` remains at `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

This is diagnostic progress, not accuracy acceptance. `ANALYSIS_VERSION` is now `6` so cached analysis created before multi-band rhythm evidence is considered stale. The next useful step is no longer more score weighting; it is a decision point between substantially deeper beat-tracking work and a dependency/licensing ADR for a dedicated rhythm-analysis library.

## Temporal Tempo-State Spike

ADR-013's first in-house spike stage adds a temporal tempo-state diagnostic over overlapping 12-second windows. Each candidate now receives `state`, which measures whether local tempo candidates and beat-grid support continue to favor that BPM over time instead of only in the whole-track aggregate.

The benchmark candidate column now includes `state`, for example:

```text
153.993@0.715[grid=0.592,beat=0.107,contrast=0.608,stable=0.856,sections=0.993,bands=0.724,state=0.606,amb=y]
```

The five-track private manifest still remains `1/5`, but the failure shape changed:

- `ffawty - Stay Home` moved away from the `77.005200` half-tempo candidate, but now picks `159.100328`; nearest candidate remains `153.992705` for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still picks `94.897379`; nearest candidate remains `77.870544` for expected `76.0`.
- `Lil Tecca - Down With Me` remains at `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

This is still not accuracy acceptance. `ANALYSIS_VERSION` is now `7` so cached analysis created before temporal tempo-state scoring is considered stale. The next useful decision is whether to continue deeper in-house beat-tracking work or approve a non-production aubio comparison spike.

## Non-Production aubio Comparison

ADR-013's second spike path now has a diagnostic harness named `aubio_compare`. It does not add aubio as a Rust dependency and does not ship aubio with the app. Instead, it shells out to a locally installed `aubiotrack` command and estimates BPM from its beat timestamps.

Run:

```sh
cargo run --offline --bin aubio_compare -- --manifest ~/Djapp-BPM-Test/private-bpm.local.tsv
```

Or pass an explicit command path:

```sh
cargo run --offline --bin aubio_compare -- --manifest ~/Djapp-BPM-Test/private-bpm.local.tsv --aubiotrack /path/to/aubiotrack
```

The output is CSV:

- `label`
- `expected_bpm`
- `aubio_bpm`
- `error_percent`
- `accepted`
- `beat_count`
- `interval_stability`
- `allowed`
- `notes`

This comparison is only meant to answer whether aubio's rhythm family performs materially better on the same private manifest. It does not approve aubio for production use, does not change `ANALYSIS_VERSION`, and does not enable Sync or AutoMix.

After installing aubio `0.4.9` with Homebrew, the default `aubiotrack` comparison accepted `0/5` exact BPMs:

- `ffawty - Stay Home`: `154.178230` for expected `151.0`, `2.104788%` error.
- `Lil Gnar - Welcome 2 Da Game`: `111.093624` for expected `76.0`, `46.175821%` error.
- `Lil Tecca - Down With Me`: `116.671593` for expected `115.0`, `1.453559%` error.
- `U Fancy`: `150.718172` for expected `148.0`, `1.836603%` error.
- `LUCKI - MORE THAN EVER`: `102.670107` for expected `152.0`, `32.453877%` error.

This means default aubio is not a drop-in answer for the current private manifest. It may still be useful for additional diagnostic comparison with other `aubiotrack` onset methods or thresholds, but the first result does not justify adding aubio to production.

## Comb-Filter / Tempogram-Style Spike

ADR-013's next in-house stage adds a scratch-built comb-filter/tempogram-style signal. Candidate generation now includes comb-filter peaks over the onset envelope, and each candidate records `comb`, a repeated-beat support score.

The benchmark candidate column now includes `comb`, for example:

```text
153.993@0.665[grid=0.516,beat=0.107,contrast=0.608,stable=0.856,sections=0.993,bands=0.724,state=0.606,comb=0.107,amb=y]
```

The five-track private manifest remains `1/5`:

- `ffawty - Stay Home` still picks `159.100328`; nearest candidate remains `153.992705` for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still picks `94.897379`; nearest candidate remains `77.870544` for expected `76.0`.
- `Lil Tecca - Down With Me` remains at `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

The `comb` values are generally low and similar across competing candidates on these tracks, so this stage did not resolve the ranking problem. `ANALYSIS_VERSION` is now `8` so cached analysis created before comb-filter scoring is considered stale.

## Dynamic Beat-Sequence Spike

ADR-013's next in-house stage adds a scratch-built dynamic-programming beat-sequence signal over onset peaks. Instead of only asking whether a fixed grid lines up with average onset strength, this pass asks whether each candidate BPM can form a coherent beat chain through the track.

The benchmark candidate column now includes `seq`, for example:

```text
153.993@0.675[grid=0.531,beat=0.107,contrast=0.608,stable=0.856,sections=0.993,bands=0.724,state=0.606,comb=0.107,seq=0.566,amb=y]
```

The five-track private manifest still remains `1/5`:

- `ffawty - Stay Home` regressed to the `77.005200` half-tempo candidate; nearest candidate remains `153.992705` for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still picks `94.897379`; nearest candidate remains `77.870544` for expected `76.0`.
- `Lil Tecca - Down With Me` remains at `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

This is an important negative result. Coherent beat chains can exist at both the reference tempo and half/double interpretations, so `seq` explains why some wrong candidates look plausible but does not solve tempo-octave selection by itself. `ANALYSIS_VERSION` is now `9` so cached analysis created before beat-sequence scoring is considered stale.

The next useful in-house work should separate these concerns:

- beat-chain existence: does this BPM produce a stable sequence?
- tempo-octave selection: should the DJ-facing BPM be the half, normal, or double interpretation?
- manual correction UX: can the owner quickly correct BPM when the algorithm remains uncertain?

## Tempo-Octave Resolver Spike

ADR-013's next in-house stage adds an explicit tempo-octave resolver after normal candidate scoring. The resolver does not use genre labels or force trap/hip-hop into a fixed range. It compares half/double-related candidates using the existing evidence plus time-consistent support, so a coherent half-tempo chain does not automatically beat a better-supported double-tempo candidate.

The benchmark candidate column now includes `oct`, for example:

```text
153.993@0.742[grid=0.531,beat=0.107,contrast=0.608,stable=0.856,sections=0.993,bands=0.724,state=0.606,comb=0.107,seq=0.566,oct=0.742,amb=n]
```

The five-track private manifest still remains `1/5`, but the failure shape improved:

- `ffawty - Stay Home` now selects `153.992705` instead of the `77.005200` half-tempo candidate; it remains outside the exact gate for expected `151.0`.
- `Lil Gnar - Welcome 2 Da Game` still picks `94.897379`; nearest candidate remains `77.870544` for expected `76.0`.
- `Lil Tecca - Down With Me` now picks `120.995512`; nearest candidate is still `118.884973` for expected `115.0`.
- `U Fancy` remains at `151.915836` for expected `148.0`.
- `LUCKI - MORE THAN EVER` remains accepted at `152.525283` for expected `152.0`.

This stage solves part of the half/double selection problem without adding a dependency, but it does not solve candidate precision or the hardest wrong-family ranking. `ANALYSIS_VERSION` is now `10` so cached analysis created before tempo-octave resolver scoring is considered stale.

## Candidate Precision Refinement Spike

ADR-013's next in-house stage adds bounded local BPM refinement around each candidate. After the analyzer has found a plausible tempo neighborhood, it searches a small range around that BPM and keeps the refined value only when beat-grid evidence improves. This addresses cases where the analyzer picked the right tempo family but landed a few percent away from the reference.

The benchmark candidate column now includes `fit`, the percent shift applied by the precision pass:

```text
151.298@0.759[grid=0.588,beat=0.241,contrast=0.862,stable=0.819,sections=0.959,bands=0.782,state=0.581,comb=0.242,seq=0.578,oct=0.759,fit=-1.75,amb=n]
```

The five-track private manifest improved from `1/5` to `4/5`:

- `ffawty - Stay Home` now measures `151.297833` against expected `151.0`, accepted at `0.197240%`.
- `Lil Tecca - Down With Me` now measures `115.140096` against expected `115.0`, accepted at `0.121823%`.
- `U Fancy` now measures `148.193898` against expected `148.0`, accepted at `0.131012%`.
- `LUCKI - MORE THAN EVER` now measures `151.991445` against expected `152.0`, accepted at `0.005628%`.
- `Lil Gnar - Welcome 2 Da Game` still fails: the correct-neighborhood candidate `75.417621` is present and within the exact gate for expected `76.0`, but the analyzer ranks `94.233097` first.

This is the strongest in-house result so far, but it is still not Sync or AutoMix acceptance. The current acceptance is `80%`, below the `90%` gate, and the benchmark is still smaller than the required representative set. `ANALYSIS_VERSION` is now `11` so cached analysis created before precision refinement is considered stale.
