# Sample-Rate Conversion

Rubato 3.0.0 converts decoded track PCM to the active engine/output rate on decoder workers. The CoreAudio callback receives only engine-rate `f32` PCM blocks.

## Behavior

- Matching source and engine rates bypass Rubato.
- Different rates use Rubato's synchronous FFT fixed-input resampler.
- Initial filter delay is removed from emitted audio.
- EOF pumps the filter tail and trims output to the mathematically expected frame count.
- Seek resets decoder and resampler state.
- Track replacement constructs a new converter for the active engine rate.
- Conversion, adapter creation, and buffer allocation occur on decoder workers, never in the audio callback.

## Fixtures

- `tone.wav`: stereo 44.1 kHz.
- `tone-48k.wav`: stereo 48 kHz.
- `tone-96k.wav`: stereo 96 kHz.

All are original three-second synthetic 440 Hz tones.

## Automated Results

- 48 kHz to 44.1 kHz produces 132,300 frames.
- 96 kHz to 48 kHz produces 144,000 frames.
- Samples remain finite and channels are preserved.
- Matching-rate bypass works.
- Seek resets converter state and produces the expected remaining duration from the decoder's actual seek boundary.

## Apple M3 Results

On macOS 26.4.1, the two-deck engine simultaneously converted:

- Deck A: 48 kHz WAV to the 44.1 kHz CoreAudio output.
- Deck B: 96 kHz WAV to the 44.1 kHz CoreAudio output.

Both decks emitted exactly 132,300 frames and reached EOF. The 261-callback run reported zero underflows, clipped samples, recycling failures, stream errors, or worker errors.

A separate 48 kHz seek test started from the decoder's 1.5-second seek boundary, emitted 66,680 engine-rate frames, and reported zero underflows or errors.

All hardware tests used gain `0.0` and were silent.

## Follow-Up Optimization

Rubato processing is isolated from the real-time callback, but the worker currently creates adapter/output storage while converting chunks. Reusing those allocations is a future performance optimization before large-library and multi-hour benchmarks.

