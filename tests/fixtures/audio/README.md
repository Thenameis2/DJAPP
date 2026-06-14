# Audio Decoder Fixtures

These files are three-second, stereo synthetic 440 Hz tones generated specifically for this project. They contain no third-party music. Most are 44.1 kHz; `tone-48k.wav` and `tone-96k.wav` exercise sample-rate conversion.

Regenerate them from the repository root with:

```sh
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a pcm_s16le tests/fixtures/audio/tone.wav
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a pcm_s16be tests/fixtures/audio/tone.aiff
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a flac tests/fixtures/audio/tone.flac
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a libmp3lame -q:a 4 -metadata title="Decoder Fixture" -metadata artist="DJ App Tests" tests/fixtures/audio/tone.mp3
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=44100:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a aac -b:a 128k -metadata title="Decoder Fixture" -metadata artist="DJ App Tests" tests/fixtures/audio/tone.m4a
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a pcm_s16le tests/fixtures/audio/tone-48k.wav
ffmpeg -y -f lavfi -i "sine=frequency=440:sample_rate=96000:duration=3" -filter_complex "[0:a]pan=stereo|c0=c0|c1=c0" -c:a pcm_s16le tests/fixtures/audio/tone-96k.wav
```

`corrupt.mp3` is a short ASCII marker used to verify graceful rejection of invalid media.
