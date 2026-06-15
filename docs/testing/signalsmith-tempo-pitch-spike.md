# Signalsmith Tempo And Pitch Spike

## Scope

This spike originally evaluated `signalsmith-stretch` 0.1.3 on the target Apple M3 without connecting it to production paths. Production integration was subsequently approved; this document preserves the isolated measurements.

Run the complete synthetic evaluation with:

```sh
cargo run --release --bin djapp-signalsmith-spike -- --soak-seconds 1800
```

## Build Findings

- Target: arm64 Apple M3, macOS 26.4.1, Rust 1.96.
- The wrapper compiled without patches using AppleClang, C++14, Bindgen, and the installed libclang.
- The spike executable is a 677,336-byte arm64 Mach-O and dynamically links only system `libc++` and `libSystem`.
- The spike added 27 locked transitive build/runtime packages. The dependency is now used by the production engine after separate owner approval.
- The crate is MIT licensed and embeds the upstream MIT-licensed Signalsmith implementation.

## Measured Results

All measurements use stereo 48 kHz audio and 512-frame processing blocks.

| Preset | Input latency | Output latency | Total |
| --- | ---: | ---: | ---: |
| Default | 2,880 frames | 2,880 frames | 120 ms |
| Cheaper | 2,400 frames | 4,320 frames | 140 ms |

Release tempo tests at `0.75x`, `0.8x`, `1.0x`, `1.25x`, and `1.5x` consumed the intended input/output ratio to within the harness's one-frame rounding tolerance. Both presets produced finite output and remained substantially faster than real time.

The default preset's worst measured pitch error was `+6.69` cents. The cheaper preset's worst error was `+9.37` cents. Both pass the 20-cent synthetic acceptance limit at `-12`, `-7`, `0`, `+7`, and `+12` semitones.

Reset/automation stress completed 400 parameter-change blocks, ten resets, and an EOF flush for each preset without a panic or non-finite sample.

The release two-deck soak processed 1,800 seconds per deck in 15.011 seconds. It observed 23 individual processing calls longer than one 512-frame period, with a 47.903 ms maximum. The processor's 120 ms startup buffer absorbed those host-scheduling spikes, so the buffered simulation reported zero underflows. Production integration must still use a bounded worker queue and must never invoke Signalsmith from a CoreAudio callback.

## Debug Observation

Unoptimized default-preset processing generated deadline misses, while the cheaper preset did not. Debug performance is not an acceptance target, but this reinforces the worker-only architecture and the need for visible queue-health telemetry.

## Quality Check

Synthetic accuracy and stability pass. Human listening remains required for percussion, vocals, bass-heavy material, and sustained harmonic material at the tempo and pitch limits. The harness cannot establish whether transient smearing, phasing, or vocal artifacts are acceptable to the owner.

## Recommendation

Accept `signalsmith-stretch` 0.1.3 as the production candidate behind a project-owned `TempoProcessor` interface, initially using the default preset. Its CPU headroom and pitch accuracy are strong on the target M3, and the default preset has lower reported total latency than the cheaper preset.

Production approval should require:

1. Keeping native construction, processing, reset, seek, flush, and destruction on one deck worker.
2. Preallocating and recycling buffers through bounded queues.
3. Compensating the audible timeline and master/cue routing for the reported processor latency.
4. Exposing queue starvation, reset, and processor-failure telemetry.
5. Completing the owner listening check before declaring audio quality accepted.

Rubber Band remains unnecessary and is not approved.
