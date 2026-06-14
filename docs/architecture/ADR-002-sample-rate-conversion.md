# ADR-002: Sample-Rate Conversion

- Status: Approved by owner on 2026-06-13
- Date: 2026-06-13
- Scope: Converting decoded track PCM to the active engine/output sample rate

## Context

The one- and two-deck engines currently require every track's sample rate to match the CoreAudio output sample rate. Real music libraries commonly mix 44.1 kHz, 48 kHz, 88.2 kHz, and 96 kHz files, so this restriction blocks normal use.

Sample-rate conversion is separate from DJ tempo shifting and key lock. This decision handles fixed source-rate to engine-rate conversion only. Time-stretching remains governed by ADR-001's later Signalsmith prototype.

## Recommendation

Approve Rubato 3.0.0 as a production dependency with its default FFT resampler support and logging disabled.

- License: MIT.
- Platform: Pure Rust with AArch64 NEON support for asynchronous sinc processing and RustFFT acceleration for synchronous FFT processing.
- Toolchain: Requires Rust 1.85 or newer, matching the project's installed Rust 1.85 toolchain.
- Maintenance: Version 3.0.0 was released on 2026-05-20 and the repository shows active development.
- Real-time behavior: `process_into_buffer` supports preallocated buffers without allocation or blocking.

Sources: [Rubato repository](https://github.com/HEnquist/rubato) and [Rubato 3.0.0 documentation](https://docs.rs/rubato/3.0.0/rubato/).

## Proposed Integration

- Use Rubato's synchronous FFT resampler for fixed track-rate to engine-rate conversion.
- Run resampling on each decoder worker, never in the CoreAudio callback.
- Aggregate variable decoder packets into fixed resampler input chunks.
- Preallocate input, output, and adapter buffers and reuse them for the track lifetime.
- Emit the existing interleaved `f32` `PcmChunk` contract at the engine sample rate.
- Reset and flush resampler state on seek and track replacement.
- Account for resampler delay when reporting transport position and reaching EOF.
- Bypass Rubato entirely when source and engine rates match.

## Alternatives

### libsamplerate

libsamplerate is mature and BSD-2-Clause licensed, but it adds C compilation, FFI, native packaging, and signing complexity. The reviewed Rust binding has a much smaller and older project history. It remains a fallback if Rubato fails quality or performance testing.

Sources: [libsamplerate repository](https://github.com/libsndfile/libsamplerate) and [rust-samplerate binding](https://github.com/Prior99/rust-samplerate).

### Project-Owned Linear Interpolation

Linear interpolation is simple but does not provide adequate anti-alias filtering for dependable music playback across arbitrary rates. It is rejected for production decoding.

### CoreAudio Conversion

Apple's native converters are mature but would increase platform-specific and unsafe integration code. They remain a fallback if the pure-Rust path cannot meet performance or quality targets.

## Acceptance Tests

- Generate original 44.1 kHz, 48 kHz, and 96 kHz fixtures.
- Convert each fixture to both 44.1 kHz and 48 kHz engine rates.
- Verify output frame counts within the documented delay/tail tolerance.
- Verify finite samples, bounded peak level, channel preservation, seek reset, EOF flushing, and no callback allocation.
- Run simultaneous mixed-rate two-deck playback through the Mac's active CoreAudio rate.
- Record CPU use, underflows, stream errors, and worker errors in a release build.

## Approval

The owner approved ADR-002 on 2026-06-13. This authorizes adding Rubato 3.0.0 and implementing fixed sample-rate conversion in the decoder workers. It does not authorize tempo shifting, pitch shifting, or time-stretching dependencies.

## Implementation Result

Rubato 3.0.0 is locked with its default FFT-resampler feature and no logging feature. A worker-side `EngineRateDecoder` bypasses matching rates and otherwise performs synchronous fixed-input conversion, initial-delay trimming, EOF tail flushing, exact output-length limiting, and seek reset.

Automated tests verified 48→44.1 kHz and 96→48 kHz frame counts, finite samples, channel preservation, bypass behavior, and seek reset. A silent Apple M3 CoreAudio test simultaneously converted 48 kHz and 96 kHz decks to the 44.1 kHz output; both produced exactly 132,300 frames with zero underflows, clipping, recycling failures, stream errors, or worker errors.

Current converter buffers are allocated on decoder workers, never in the CoreAudio callback. Reusing worker allocations remains a performance optimization before long-duration benchmarks.
