# Dual-Output Synchronization Spike

## Purpose

Measure whether the target Mac's separately exposed MacBook Pro Speakers and External Headphones outputs accumulate rendered-frame drift when both silent CoreAudio streams run concurrently. This is an experimental measurement and does not change production routing.

## Command

```sh
target/release/djapp-audio-spike \
  --dual-output-master "MacBook Pro Speakers" \
  --dual-output-cue "External Headphones" \
  --run-seconds 1800
```

The probe requires different devices with matching nominal sample rates. It opens their default configurations, renders silence, waits two seconds for warm-up, records the relative rendered-frame baseline, and samples both atomic frame counters once per second. Runs longer than two minutes print one observation per minute.

## Apple M3 Result

Date: 2026-06-14

- Master: MacBook Pro Speakers, stereo 44.1 kHz `f32`.
- Cue candidate: External Headphones, stereo 44.1 kHz `f32`.
- Measurement duration: 1,800 seconds after warm-up.
- Master callbacks: 155,853.
- Cue callbacks: 155,851.
- Final sampled drift from baseline: 0 frames.
- Maximum observed absolute drift from baseline: 512 frames, approximately 11.6 ms.
- Master stream errors: 0.
- Cue stream errors: 0.

The relative sampled position alternated between the baseline and one 512-frame callback quantum behind it. There was no accumulating trend over 30 minutes. This is evidence that these routes advance at effectively synchronized rates on this Mac, or that CoreAudio compensates them sufficiently for this silent frame-count test.

## Limitations

- Frame counters do not measure acoustic latency or the fixed latency difference between speakers and headphones.
- Silence does not test shared audio-buffer fan-out, underflow policy, or real decoder/mixer load.
- Headphone or speaker disconnection/reconnection was not exercised during the uninterrupted run.
- This result applies to the tested Mac and routes; it does not prove all separate-device combinations are stable.
- Production routing still needs bounded shared buffering, latency-offset handling, device-loss recovery, and loaded two-deck testing.

## Repeating The Test

Use exact names or serialized CPAL device UIDs. Keep both devices connected for the full run. Any stream error, steadily increasing drift, or nominal sample-rate mismatch fails the viability check.
