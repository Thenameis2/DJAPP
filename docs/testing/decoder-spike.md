# Decoder Spike

The decoder spike validates local-file probing, metadata, seeking, and bounded conversion to the engine's internal interleaved `f32` PCM format.

## Supported Test Matrix

| Fixture | Container/codec | Result |
| --- | --- | --- |
| `tone.wav` | WAV / signed 16-bit PCM | Decode and seek tested |
| `tone.aiff` | AIFF / signed 16-bit PCM | Decode and seek tested |
| `tone.flac` | FLAC | Decode and seek tested |
| `tone.mp3` | MP3 with ID3 metadata | Decode, metadata, and seek tested |
| `tone.m4a` | MP4/M4A with AAC-LC and metadata | Decode, metadata, and seek tested |
| `corrupt.mp3` | Invalid marker file | Graceful rejection tested |

All valid fixtures are original three-second, stereo, 44.1 kHz synthetic tones. Their generation commands are recorded in `tests/fixtures/audio/README.md`.

## Manual Inspection

```sh
cargo run --release -- --inspect-media tests/fixtures/audio/tone.m4a
cargo run --release -- --inspect-media tests/fixtures/audio/tone.m4a --seek-seconds 1.5
```

The command prints normalized media information and the first bounded PCM chunk. It does not play the file.

## Decoder Contract

- Probe by file content with the extension supplied only as a hint.
- Select the default decodable audio track.
- Return stable title, artist, codec, channel, sample-rate, and duration fields.
- Convert decoded packets to finite, interleaved `f32` samples.
- Yield bounded chunks rather than loading an entire track into the playback path.
- Reset decoder state after every seek.
- Reject invalid seek values and corrupt or unsupported files without panicking.

## Known Limits

- The current AAC feature covers AAC-LC. HE-AAC and HE-AACv2 are not supported by Symphonia 0.6.0.
- Synthetic fixtures prove the integration path, not compatibility with every encoder or malformed real-world file.
- Gapless delay and padding behavior needs a dedicated fixture before one-deck playback acceptance.
- Metadata currently exposes normalized title and artist only. Album, genre, artwork, and additional fields belong to the later library-scanning step.

