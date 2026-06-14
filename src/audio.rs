use std::{
    error::Error,
    f32::consts::TAU,
    fmt,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};

const COMMAND_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AudioCommand {
    SetGain(f32),
    Stop,
}

#[derive(Clone, Copy, Debug)]
pub struct ProbeOptions {
    pub tone_hz: Option<f32>,
    pub initial_gain: f32,
}

pub struct AudioProbe {
    host: cpal::Host,
}

impl AudioProbe {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            host: cpal::default_host(),
        })
    }

    pub fn print_devices(&self) -> Result<(), Box<dyn Error>> {
        println!("Audio host: {}", self.host.id().name());

        let default_input = self.host.default_input_device();
        let default_output = self.host.default_output_device();
        println!(
            "Default input: {}",
            device_name(default_input.as_ref()).unwrap_or_else(|| "None".to_string())
        );
        println!(
            "Default output: {}",
            device_name(default_output.as_ref()).unwrap_or_else(|| "None".to_string())
        );

        println!("\nOutput devices:");
        for (index, device) in self.host.output_devices()?.enumerate() {
            println!("  [{index}] {}", device.description()?);
            match device.default_output_config() {
                Ok(config) => println!("      default: {config:?}"),
                Err(error) => println!("      default config unavailable: {error}"),
            }
        }

        println!("\nInput devices:");
        for (index, device) in self.host.input_devices()?.enumerate() {
            println!("  [{index}] {}", device.description()?);
            match device.default_input_config() {
                Ok(config) => println!("      default: {config:?}"),
                Err(error) => println!("      default config unavailable: {error}"),
            }
        }

        Ok(())
    }

    pub fn start_default_output(
        &self,
        options: ProbeOptions,
    ) -> Result<RunningProbe, Box<dyn Error>> {
        let device = self
            .host
            .default_output_device()
            .ok_or("no default output device")?;
        let supported = device.default_output_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let (producer, consumer) = RingBuffer::new(COMMAND_QUEUE_CAPACITY);
        let metrics = Arc::new(CallbackMetrics::default());

        let stream = match sample_format {
            SampleFormat::F32 => {
                build_stream::<f32>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::F64 => {
                build_stream::<f64>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::I8 => build_stream::<i8>(&device, &config, consumer, options, &metrics)?,
            SampleFormat::I16 => {
                build_stream::<i16>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::I24 => {
                build_stream::<cpal::I24>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::I32 => {
                build_stream::<i32>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::I64 => {
                build_stream::<i64>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::U8 => build_stream::<u8>(&device, &config, consumer, options, &metrics)?,
            SampleFormat::U16 => {
                build_stream::<u16>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::U32 => {
                build_stream::<u32>(&device, &config, consumer, options, &metrics)?
            }
            SampleFormat::U64 => {
                build_stream::<u64>(&device, &config, consumer, options, &metrics)?
            }
            other => return Err(format!("unsupported output sample format: {other:?}").into()),
        };

        stream.play()?;
        Ok(RunningProbe {
            producer,
            stream,
            metrics,
        })
    }
}

pub struct RunningProbe {
    producer: Producer<AudioCommand>,
    stream: Stream,
    metrics: Arc<CallbackMetrics>,
}

impl RunningProbe {
    pub fn try_send(&mut self, command: AudioCommand) -> Result<(), CommandQueueFull> {
        self.producer.push(command).map_err(|_| CommandQueueFull)
    }

    pub fn finish(self) -> ProbeReport {
        let deadline = Instant::now() + Duration::from_millis(250);
        while !self.metrics.stopped.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }

        drop(self.stream);
        ProbeReport {
            callbacks: self.metrics.callbacks.load(Ordering::Relaxed),
            samples: self.metrics.samples.load(Ordering::Relaxed),
            commands: self.metrics.commands.load(Ordering::Relaxed),
            stream_errors: self.metrics.stream_errors.load(Ordering::Relaxed),
            stopped: self.metrics.stopped.load(Ordering::Acquire),
        }
    }
}

#[derive(Debug)]
pub struct CommandQueueFull;

impl fmt::Display for CommandQueueFull {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("audio command queue is full")
    }
}

impl Error for CommandQueueFull {}

#[derive(Debug, PartialEq)]
pub struct ProbeReport {
    pub callbacks: u64,
    pub samples: u64,
    pub commands: u64,
    pub stream_errors: u64,
    pub stopped: bool,
}

impl ProbeReport {
    pub fn summary(&self) -> String {
        format!(
            "Callback report: callbacks={}, samples={}, commands={}, stream_errors={}, stopped={}",
            self.callbacks, self.samples, self.commands, self.stream_errors, self.stopped
        )
    }
}

#[derive(Default)]
struct CallbackMetrics {
    callbacks: AtomicU64,
    samples: AtomicU64,
    commands: AtomicU64,
    stream_errors: AtomicU64,
    stopped: AtomicBool,
}

struct RenderState {
    gain: f32,
    phase: f32,
    phase_step: f32,
    stopped: bool,
}

impl RenderState {
    fn new(options: ProbeOptions, sample_rate: u32) -> Self {
        Self {
            gain: options.initial_gain.clamp(0.0, 0.25),
            phase: 0.0,
            phase_step: options
                .tone_hz
                .map(|frequency| TAU * frequency / sample_rate as f32)
                .unwrap_or(0.0),
            stopped: false,
        }
    }

    fn apply_pending(&mut self, consumer: &mut Consumer<AudioCommand>) -> u64 {
        let mut count = 0;
        while let Ok(command) = consumer.pop() {
            count += 1;
            match command {
                AudioCommand::SetGain(gain) => self.gain = gain.clamp(0.0, 0.25),
                AudioCommand::Stop => self.stopped = true,
            }
        }
        count
    }

    fn next_sample(&mut self) -> f32 {
        if self.stopped || self.phase_step == 0.0 {
            return 0.0;
        }

        let sample = self.phase.sin() * self.gain;
        self.phase += self.phase_step;
        if self.phase >= TAU {
            self.phase -= TAU;
        }
        sample
    }
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mut consumer: Consumer<AudioCommand>,
    options: ProbeOptions,
    metrics: &Arc<CallbackMetrics>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let callback_metrics = Arc::clone(metrics);
    let error_metrics = Arc::clone(metrics);
    let mut state = RenderState::new(options, config.sample_rate);

    device.build_output_stream(
        *config,
        move |output: &mut [T], _| {
            let command_count = state.apply_pending(&mut consumer);
            callback_metrics
                .commands
                .fetch_add(command_count, Ordering::Relaxed);
            callback_metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            callback_metrics
                .samples
                .fetch_add(output.len() as u64, Ordering::Relaxed);

            for sample in output {
                *sample = T::from_sample(state.next_sample());
            }

            if state.stopped {
                callback_metrics.stopped.store(true, Ordering::Release);
            }
        },
        move |_error| {
            error_metrics.stream_errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
}

fn device_name(device: Option<&Device>) -> Option<String> {
    device.and_then(|device| {
        device
            .description()
            .ok()
            .map(|description| description.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_update_render_state_without_blocking() {
        let (mut producer, mut consumer) = RingBuffer::new(4);
        let mut state = RenderState::new(
            ProbeOptions {
                tone_hz: Some(440.0),
                initial_gain: 0.1,
            },
            48_000,
        );

        producer.push(AudioCommand::SetGain(0.2)).unwrap();
        producer.push(AudioCommand::Stop).unwrap();

        assert_eq!(state.apply_pending(&mut consumer), 2);
        assert_eq!(state.gain, 0.2);
        assert!(state.stopped);
        assert_eq!(state.next_sample(), 0.0);
    }

    #[test]
    fn gain_is_clamped() {
        let (_producer, mut consumer) = RingBuffer::new(1);
        let mut state = RenderState::new(
            ProbeOptions {
                tone_hz: None,
                initial_gain: 10.0,
            },
            48_000,
        );

        assert_eq!(state.apply_pending(&mut consumer), 0);
        assert_eq!(state.gain, 0.25);
    }

    #[test]
    fn report_summary_contains_health_counters() {
        let report = ProbeReport {
            callbacks: 10,
            samples: 5_120,
            commands: 2,
            stream_errors: 0,
            stopped: true,
        };

        assert_eq!(
            report.summary(),
            "Callback report: callbacks=10, samples=5120, commands=2, stream_errors=0, stopped=true"
        );
    }
}
