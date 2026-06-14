# Audio Spike Manual Test

The audio spike validates device enumeration, a CPAL output callback, bounded `rtrb` commands, and basic callback health counters. It is not the DJ application or final audio engine.

## Commands

List devices without opening an output stream:

```sh
cargo run --release
```

Run a five-second silent callback test:

```sh
cargo run --release -- --run-seconds 5
```

Run a quiet 440 Hz audible test:

```sh
cargo run --release -- --run-seconds 5 --tone-hz 440 --gain 0.05
```

Keep speaker volume low before using the audible test. The command limits generated tone gain to `0.25`.

## Pass Conditions

- The expected CoreAudio devices are listed.
- The default output device and configuration are reported.
- The callback report shows a nonzero callback and sample count.
- `stream_errors=0`.
- `commands=2` and `stopped=true` for runs longer than one second.
- Device connection or permission failures produce an error instead of a panic.

