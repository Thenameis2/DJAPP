use std::{
    error::Error,
    fmt,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Device, ErrorKind, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::{
    deck::{
        spawn_decoder_worker, DeckMediaInfo, DeckMetrics, DeckSnapshot, DeckState, DecodedBlock,
        RenderCommand, RenderState, WorkerCommand, AUDIO_QUEUE_CAPACITY, CONTROL_QUEUE_CAPACITY,
        RECYCLE_QUEUE_CAPACITY,
    },
    device::{preferred_output_config, resolve_output_device},
    media::{decode::MediaDecoder, resample::EngineRateDecoder},
};

const MIXER_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckId {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum MixerCommand {
    Crossfader(f32),
    MasterGain(f32),
    Cue(DeckId, bool),
    CueBlend(f32),
    CueGain(f32),
}

struct DeckPipeline {
    control: Producer<RenderCommand>,
    worker_control: Option<mpsc::Sender<WorkerCommand>>,
    worker: Option<JoinHandle<()>>,
    pending_worker_queues: Option<(Producer<DecodedBlock>, Consumer<Vec<f32>>)>,
    metrics: Arc<DeckMetrics>,
    worker_error: Arc<Mutex<Option<String>>>,
    generation: u64,
    media: Option<DeckMediaInfo>,
    output_sample_rate: u32,
    position_base_frames: u64,
}

impl DeckPipeline {
    fn empty(output_sample_rate: u32) -> (Self, DeckRender) {
        let (control, render_commands) = RingBuffer::new(CONTROL_QUEUE_CAPACITY);
        let (audio_blocks, render_blocks) = RingBuffer::new(AUDIO_QUEUE_CAPACITY);
        let (recycle_buffers, recycled_buffers) = RingBuffer::new(RECYCLE_QUEUE_CAPACITY);
        let metrics = Arc::new(DeckMetrics::new(1));
        let worker_error = Arc::new(Mutex::new(None));

        (
            Self {
                control,
                worker_control: None,
                worker: None,
                pending_worker_queues: Some((audio_blocks, recycled_buffers)),
                metrics: Arc::clone(&metrics),
                worker_error,
                generation: 1,
                media: None,
                output_sample_rate,
                position_base_frames: 0,
            },
            DeckRender {
                commands: render_commands,
                blocks: render_blocks,
                recycle: recycle_buffers,
                state: RenderState::new(1.0),
                metrics,
            },
        )
    }

    fn load(&mut self, path: impl AsRef<Path>, autoplay: bool) -> Result<(), Box<dyn Error>> {
        let path = path.as_ref().to_path_buf();
        let decoder = MediaDecoder::open(&path)?;
        let media = DeckMediaInfo {
            path: path.clone(),
            sample_rate: decoder.info().sample_rate,
            output_sample_rate: self.output_sample_rate,
            channels: decoder.info().channels,
            duration_seconds: decoder.info().duration_seconds,
        };
        let decoder = EngineRateDecoder::new(decoder, self.output_sample_rate)?;

        self.generation += 1;
        self.metrics
            .active_generation
            .store(self.generation, Ordering::Release);
        self.send(RenderCommand::Reset {
            generation: self.generation,
            playing: autoplay,
        })?;

        if let Some(worker_control) = &self.worker_control {
            worker_control
                .send(WorkerCommand::Replace {
                    path,
                    generation: self.generation,
                })
                .map_err(|_| MixerControlError::WorkerStopped)?;
        } else {
            let (audio_blocks, recycled_buffers) = self
                .pending_worker_queues
                .take()
                .ok_or(MixerControlError::WorkerStopped)?;
            let (worker_control, worker_commands) = mpsc::channel();
            self.worker = Some(spawn_decoder_worker(
                decoder,
                self.generation,
                audio_blocks,
                recycled_buffers,
                worker_commands,
                Arc::clone(&self.metrics),
                Arc::clone(&self.worker_error),
            ));
            self.worker_control = Some(worker_control);
        }
        if let Ok(mut error) = self.worker_error.lock() {
            *error = None;
        }
        self.media = Some(media);
        self.position_base_frames = 0;
        Ok(())
    }

    fn send(&mut self, command: RenderCommand) -> Result<(), MixerControlError> {
        self.control
            .push(command)
            .map_err(|_| MixerControlError::QueueFull)
    }

    fn seek(&mut self, seconds: f64, resume: bool) -> Result<(), MixerControlError> {
        let worker_control = self
            .worker_control
            .as_ref()
            .ok_or(MixerControlError::DeckUnloaded)?
            .clone();
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(MixerControlError::InvalidValue);
        }
        self.position_base_frames = (seconds * f64::from(self.output_sample_rate)).round() as u64;
        self.generation += 1;
        self.metrics
            .active_generation
            .store(self.generation, Ordering::Release);
        self.send(RenderCommand::Reset {
            generation: self.generation,
            playing: resume,
        })?;
        worker_control
            .send(WorkerCommand::Seek {
                seconds,
                generation: self.generation,
            })
            .map_err(|_| MixerControlError::WorkerStopped)
    }

    fn snapshot(&self) -> DeckSnapshot {
        let generation = self.metrics.active_generation.load(Ordering::Acquire);
        let ended = self.metrics.ended_generation.load(Ordering::Acquire) == generation;
        let playing = self.metrics.playing.load(Ordering::Acquire);
        DeckSnapshot {
            state: if ended {
                DeckState::Ended
            } else if playing {
                DeckState::Playing
            } else {
                DeckState::Paused
            },
            position_frames: self.position_base_frames
                + self.metrics.position_frames.load(Ordering::Relaxed),
            rendered_frames: self.metrics.rendered_frames.load(Ordering::Relaxed),
            callbacks: self.metrics.callbacks.load(Ordering::Relaxed),
            underflow_callbacks: self.metrics.underflows.load(Ordering::Relaxed),
            stale_blocks: self.metrics.stale_blocks.load(Ordering::Relaxed),
            recycle_failures: self.metrics.recycle_failures.load(Ordering::Relaxed),
            stream_errors: self.metrics.stream_errors.load(Ordering::Relaxed),
            generation,
            worker_error: self
                .worker_error
                .lock()
                .ok()
                .and_then(|value| value.clone()),
        }
    }

    fn shutdown(&mut self) {
        if let Some(worker_control) = &self.worker_control {
            let _ = worker_control.send(WorkerCommand::Shutdown);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct DeckRender {
    commands: Consumer<RenderCommand>,
    blocks: Consumer<DecodedBlock>,
    recycle: Producer<Vec<f32>>,
    state: RenderState,
    metrics: Arc<DeckMetrics>,
}

impl DeckRender {
    fn render(&mut self, output: &mut [f32]) -> bool {
        self.state
            .apply_commands(&mut self.commands, &mut self.recycle, &self.metrics);
        self.state
            .render_frame(output, &mut self.blocks, &mut self.recycle, &self.metrics)
    }
}

pub struct MixerEngine {
    deck_a: DeckPipeline,
    deck_b: DeckPipeline,
    mixer_control: Producer<MixerCommand>,
    _stream: Stream,
    metrics: Arc<MixerMetrics>,
    cue_supported: bool,
}

impl MixerEngine {
    pub fn open_default_unloaded() -> Result<Self, Box<dyn Error>> {
        Self::open_output_device_unloaded(None)
    }

    pub fn open_output_device_unloaded(device_id: Option<&str>) -> Result<Self, Box<dyn Error>> {
        let device = resolve_output_device(device_id)?;
        let supported = preferred_output_config(&device)?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let (deck_a, render_a) = DeckPipeline::empty(config.sample_rate);
        let (deck_b, render_b) = DeckPipeline::empty(config.sample_rate);
        Self::open_with_device(
            device,
            config,
            sample_format,
            deck_a,
            deck_b,
            render_a,
            render_b,
        )
    }

    pub fn open_default(
        deck_a_path: impl AsRef<Path>,
        deck_b_path: impl AsRef<Path>,
    ) -> Result<Self, Box<dyn Error>> {
        let mut engine = Self::open_default_unloaded()?;
        engine.load_track(DeckId::A, deck_a_path, false)?;
        engine.load_track(DeckId::B, deck_b_path, false)?;
        Ok(engine)
    }

    fn open_with_device(
        device: Device,
        config: StreamConfig,
        sample_format: SampleFormat,
        deck_a: DeckPipeline,
        deck_b: DeckPipeline,
        render_a: DeckRender,
        render_b: DeckRender,
    ) -> Result<Self, Box<dyn Error>> {
        let (mixer_control, mixer_commands) = RingBuffer::new(MIXER_QUEUE_CAPACITY);
        let metrics = Arc::new(MixerMetrics::default());
        let cue_supported = config.channels >= 4;

        let stream = match sample_format {
            SampleFormat::F32 => build_mixer_stream::<f32>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::F64 => build_mixer_stream::<f64>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::I8 => build_mixer_stream::<i8>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::I16 => build_mixer_stream::<i16>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::I24 => build_mixer_stream::<cpal::I24>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::I32 => build_mixer_stream::<i32>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::I64 => build_mixer_stream::<i64>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::U8 => build_mixer_stream::<u8>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::U16 => build_mixer_stream::<u16>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::U32 => build_mixer_stream::<u32>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            SampleFormat::U64 => build_mixer_stream::<u64>(
                &device,
                &config,
                render_a,
                render_b,
                mixer_commands,
                &metrics,
            )?,
            other => return Err(format!("unsupported output sample format: {other:?}").into()),
        };

        stream.play()?;
        Ok(Self {
            deck_a,
            deck_b,
            mixer_control,
            _stream: stream,
            metrics,
            cue_supported,
        })
    }

    pub fn load_track(
        &mut self,
        deck: DeckId,
        path: impl AsRef<Path>,
        autoplay: bool,
    ) -> Result<(), Box<dyn Error>> {
        self.deck_mut(deck).load(path, autoplay)
    }

    pub fn media(&self, deck: DeckId) -> Option<&DeckMediaInfo> {
        self.deck(deck).media.as_ref()
    }

    pub fn play(&mut self, deck: DeckId) -> Result<(), MixerControlError> {
        self.ensure_loaded(deck)?;
        self.deck_mut(deck).send(RenderCommand::Play)
    }

    pub fn pause(&mut self, deck: DeckId) -> Result<(), MixerControlError> {
        self.ensure_loaded(deck)?;
        self.deck_mut(deck).send(RenderCommand::Pause)
    }

    pub fn stop(&mut self, deck: DeckId) -> Result<(), MixerControlError> {
        self.deck_mut(deck).seek(0.0, false)
    }

    pub fn seek(
        &mut self,
        deck: DeckId,
        seconds: f64,
        resume: bool,
    ) -> Result<(), MixerControlError> {
        self.deck_mut(deck).seek(seconds, resume)
    }

    pub fn set_channel_gain(&mut self, deck: DeckId, gain: f32) -> Result<(), MixerControlError> {
        self.ensure_loaded(deck)?;
        if !gain.is_finite() {
            return Err(MixerControlError::InvalidValue);
        }
        self.deck_mut(deck).send(RenderCommand::SetGain(gain))
    }

    pub fn set_crossfader(&mut self, value: f32) -> Result<(), MixerControlError> {
        if !value.is_finite() {
            return Err(MixerControlError::InvalidValue);
        }
        self.mixer_control
            .push(MixerCommand::Crossfader(value.clamp(-1.0, 1.0)))
            .map_err(|_| MixerControlError::QueueFull)
    }

    pub fn set_master_gain(&mut self, gain: f32) -> Result<(), MixerControlError> {
        if !gain.is_finite() {
            return Err(MixerControlError::InvalidValue);
        }
        self.mixer_control
            .push(MixerCommand::MasterGain(gain.clamp(0.0, 1.0)))
            .map_err(|_| MixerControlError::QueueFull)
    }

    pub fn set_cue(&mut self, deck: DeckId, enabled: bool) -> Result<(), MixerControlError> {
        if !self.cue_supported {
            return Err(MixerControlError::CueUnavailable);
        }
        self.mixer_control
            .push(MixerCommand::Cue(deck, enabled))
            .map_err(|_| MixerControlError::QueueFull)
    }

    pub fn set_cue_blend(&mut self, value: f32) -> Result<(), MixerControlError> {
        if !self.cue_supported {
            return Err(MixerControlError::CueUnavailable);
        }
        if !value.is_finite() {
            return Err(MixerControlError::InvalidValue);
        }
        self.mixer_control
            .push(MixerCommand::CueBlend(value.clamp(-1.0, 1.0)))
            .map_err(|_| MixerControlError::QueueFull)
    }

    pub fn set_cue_gain(&mut self, gain: f32) -> Result<(), MixerControlError> {
        if !self.cue_supported {
            return Err(MixerControlError::CueUnavailable);
        }
        if !gain.is_finite() {
            return Err(MixerControlError::InvalidValue);
        }
        self.mixer_control
            .push(MixerCommand::CueGain(gain.clamp(0.0, 1.0)))
            .map_err(|_| MixerControlError::QueueFull)
    }

    pub fn cue_supported(&self) -> bool {
        self.cue_supported
    }

    pub fn snapshot(&self) -> MixerSnapshot {
        MixerSnapshot {
            deck_a: self.deck_a.snapshot(),
            deck_b: self.deck_b.snapshot(),
            callbacks: self.metrics.callbacks.load(Ordering::Relaxed),
            clipped_samples: self.metrics.clipped_samples.load(Ordering::Relaxed),
            stream_errors: self.metrics.stream_errors.load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(mut self) -> MixerSnapshot {
        self.deck_a.shutdown();
        self.deck_b.shutdown();
        self.snapshot()
    }

    fn deck(&self, deck: DeckId) -> &DeckPipeline {
        match deck {
            DeckId::A => &self.deck_a,
            DeckId::B => &self.deck_b,
        }
    }

    fn deck_mut(&mut self, deck: DeckId) -> &mut DeckPipeline {
        match deck {
            DeckId::A => &mut self.deck_a,
            DeckId::B => &mut self.deck_b,
        }
    }

    fn ensure_loaded(&self, deck: DeckId) -> Result<(), MixerControlError> {
        self.deck(deck)
            .media
            .as_ref()
            .map(|_| ())
            .ok_or(MixerControlError::DeckUnloaded)
    }
}

impl Drop for MixerEngine {
    fn drop(&mut self) {
        self.deck_a.shutdown();
        self.deck_b.shutdown();
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MixerSnapshot {
    pub deck_a: DeckSnapshot,
    pub deck_b: DeckSnapshot,
    pub callbacks: u64,
    pub clipped_samples: u64,
    pub stream_errors: u64,
}

impl MixerSnapshot {
    pub fn summary(&self) -> String {
        format!(
            "Mixer report: callbacks={}, clipped_samples={}, stream_errors={}, deck_a=[{}], deck_b=[{}]",
            self.callbacks,
            self.clipped_samples,
            self.stream_errors,
            self.deck_a.summary(),
            self.deck_b.summary()
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MixerControlError {
    DeckUnloaded,
    CueUnavailable,
    QueueFull,
    WorkerStopped,
    InvalidValue,
}

impl fmt::Display for MixerControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeckUnloaded => formatter.write_str("no track is loaded in this deck"),
            Self::CueUnavailable => formatter.write_str(
                "stereo headphone cue requires one output device with at least four channels",
            ),
            Self::QueueFull => formatter.write_str("mixer control queue is full"),
            Self::WorkerStopped => formatter.write_str("decoder worker has stopped"),
            Self::InvalidValue => formatter.write_str("mixer control value is invalid"),
        }
    }
}

impl Error for MixerControlError {}

#[derive(Default)]
struct MixerMetrics {
    callbacks: AtomicU64,
    clipped_samples: AtomicU64,
    stream_errors: AtomicU64,
}

#[derive(Clone, Copy, Debug)]
struct MixerState {
    crossfader: f32,
    crossfade_gain_a: f32,
    crossfade_gain_b: f32,
    master_gain: f32,
    cue_a: bool,
    cue_b: bool,
    cue_blend: f32,
    cue_gain: f32,
}

impl Default for MixerState {
    fn default() -> Self {
        let (crossfade_gain_a, crossfade_gain_b) = equal_power_crossfader(0.0);
        Self {
            crossfader: 0.0,
            crossfade_gain_a,
            crossfade_gain_b,
            master_gain: 1.0,
            cue_a: false,
            cue_b: false,
            cue_blend: -1.0,
            cue_gain: 0.5,
        }
    }
}

impl MixerState {
    fn apply_commands(&mut self, commands: &mut Consumer<MixerCommand>) {
        while let Ok(command) = commands.pop() {
            match command {
                MixerCommand::Crossfader(value) => {
                    self.crossfader = value.clamp(-1.0, 1.0);
                    (self.crossfade_gain_a, self.crossfade_gain_b) =
                        equal_power_crossfader(self.crossfader);
                }
                MixerCommand::MasterGain(gain) => self.master_gain = gain.clamp(0.0, 1.0),
                MixerCommand::Cue(deck, enabled) => match deck {
                    DeckId::A => self.cue_a = enabled,
                    DeckId::B => self.cue_b = enabled,
                },
                MixerCommand::CueBlend(value) => self.cue_blend = value.clamp(-1.0, 1.0),
                MixerCommand::CueGain(gain) => self.cue_gain = gain.clamp(0.0, 1.0),
            }
        }
    }

    fn gains(&self) -> (f32, f32) {
        (self.crossfade_gain_a, self.crossfade_gain_b)
    }

    fn mix(&self, deck_a: f32, deck_b: f32) -> f32 {
        let (gain_a, gain_b) = self.gains();
        (deck_a * gain_a + deck_b * gain_b) * self.master_gain
    }

    fn cue_mix(&self, deck_a: f32, deck_b: f32, master: f32) -> f32 {
        let selected = match (self.cue_a, self.cue_b) {
            (true, true) => (deck_a + deck_b) * std::f32::consts::FRAC_1_SQRT_2,
            (true, false) => deck_a,
            (false, true) => deck_b,
            (false, false) => 0.0,
        };
        let (cue_gain, master_gain) = equal_power_crossfader(self.cue_blend);
        (selected * cue_gain + master * master_gain) * self.cue_gain
    }

    fn route_frame(&self, deck_a: [f32; 2], deck_b: [f32; 2], output: &mut [f32]) {
        output.fill(0.0);
        let master = [
            self.mix(deck_a[0], deck_b[0]),
            self.mix(deck_a[1], deck_b[1]),
        ];
        if let Some(left) = output.get_mut(0) {
            *left = master[0];
        }
        if let Some(right) = output.get_mut(1) {
            *right = master[1];
        }
        if output.len() >= 4 {
            output[2] = self.cue_mix(deck_a[0], deck_b[0], master[0]);
            output[3] = self.cue_mix(deck_a[1], deck_b[1], master[1]);
        }
    }
}

pub fn equal_power_crossfader(value: f32) -> (f32, f32) {
    let normalized = (value.clamp(-1.0, 1.0) + 1.0) * 0.5;
    let angle = normalized * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

fn build_mixer_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mut deck_a: DeckRender,
    mut deck_b: DeckRender,
    mut commands: Consumer<MixerCommand>,
    metrics: &Arc<MixerMetrics>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let output_channels = usize::from(config.channels);
    if output_channels > 32 {
        return Err(cpal::Error::with_message(
            ErrorKind::UnsupportedConfig,
            "more than 32 output channels",
        ));
    }

    let callback_metrics = Arc::clone(metrics);
    let error_metrics = Arc::clone(metrics);
    let mut mixer = MixerState::default();
    let mut frame_a = [0.0_f32; 2];
    let mut frame_b = [0.0_f32; 2];
    let mut routed = [0.0_f32; 32];

    device.build_output_stream(
        *config,
        move |output: &mut [T], _| {
            callback_metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            deck_a.metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            deck_b.metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            mixer.apply_commands(&mut commands);

            let mut underflow_a = false;
            let mut underflow_b = false;
            for output_frame in output.chunks_mut(output_channels) {
                if !deck_a.render(&mut frame_a) {
                    underflow_a = true;
                }
                if !deck_b.render(&mut frame_b) {
                    underflow_b = true;
                }

                mixer.route_frame(frame_a, frame_b, &mut routed[..output_channels]);
                for (sample, mixed) in output_frame.iter_mut().zip(routed.iter()) {
                    if mixed.abs() > 1.0 {
                        callback_metrics
                            .clipped_samples
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    *sample = T::from_sample(mixed.clamp(-1.0, 1.0));
                }
            }
            if underflow_a {
                deck_a.metrics.underflows.fetch_add(1, Ordering::Relaxed);
            }
            if underflow_b {
                deck_b.metrics.underflows.fetch_add(1, Ordering::Relaxed);
            }
        },
        move |_error| {
            error_metrics.stream_errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
}

pub fn wait_until_both_ended(engine: &MixerEngine, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let snapshot = engine.snapshot();
        if snapshot.deck_a.state == DeckState::Ended && snapshot.deck_b.state == DeckState::Ended {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfader_reaches_expected_endpoints_and_center() {
        let left = equal_power_crossfader(-1.0);
        let center = equal_power_crossfader(0.0);
        let right = equal_power_crossfader(1.0);

        assert!((left.0 - 1.0).abs() < 1e-6);
        assert!(left.1.abs() < 1e-6);
        assert!((center.0 - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((center.1 - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!(right.0.abs() < 1e-6);
        assert!((right.1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mixer_applies_crossfader_and_master_gain_deterministically() {
        let mut state = MixerState {
            crossfader: -1.0,
            crossfade_gain_a: 1.0,
            crossfade_gain_b: 0.0,
            master_gain: 0.5,
            cue_a: false,
            cue_b: false,
            cue_blend: -1.0,
            cue_gain: 0.5,
        };
        assert!((state.mix(0.8, -0.8) - 0.4).abs() < 1e-6);

        state.crossfader = 1.0;
        (state.crossfade_gain_a, state.crossfade_gain_b) = equal_power_crossfader(1.0);
        assert!((state.mix(0.8, -0.8) + 0.4).abs() < 1e-6);

        state.crossfader = 0.0;
        (state.crossfade_gain_a, state.crossfade_gain_b) = equal_power_crossfader(0.0);
        assert!(state.mix(0.5, -0.5).abs() < 1e-6);
    }

    #[test]
    fn control_commands_clamp_values() {
        let (mut producer, mut consumer) = RingBuffer::new(4);
        let mut state = MixerState::default();
        producer.push(MixerCommand::Crossfader(2.0)).unwrap();
        producer.push(MixerCommand::MasterGain(-1.0)).unwrap();
        state.apply_commands(&mut consumer);
        assert_eq!(state.crossfader, 1.0);
        assert_eq!(state.master_gain, 0.0);
    }

    #[test]
    fn cue_routes_pre_crossfader_audio_to_channels_three_and_four() {
        let mut state = MixerState {
            crossfader: 1.0,
            cue_a: true,
            cue_blend: -1.0,
            cue_gain: 1.0,
            ..MixerState::default()
        };
        (state.crossfade_gain_a, state.crossfade_gain_b) = equal_power_crossfader(1.0);
        let mut output = [9.0; 6];
        state.route_frame([0.25, -0.25], [0.5, -0.5], &mut output);

        assert!((output[0] - 0.5).abs() < 1e-6);
        assert!((output[1] + 0.5).abs() < 1e-6);
        assert!((output[2] - 0.25).abs() < 1e-6);
        assert!((output[3] + 0.25).abs() < 1e-6);
        assert_eq!(output[4], 0.0);
        assert_eq!(output[5], 0.0);
    }

    #[test]
    fn master_only_routing_never_leaks_cue() {
        let state = MixerState {
            cue_a: true,
            cue_gain: 1.0,
            ..MixerState::default()
        };
        let mut output = [0.0; 2];
        state.route_frame([1.0, -1.0], [0.0, 0.0], &mut output);
        assert!((output[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((output[1] + std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }

    #[test]
    fn cue_blend_endpoints_and_center_are_equal_power() {
        let mut state = MixerState {
            cue_a: true,
            cue_gain: 1.0,
            cue_blend: -1.0,
            ..MixerState::default()
        };
        assert!((state.cue_mix(0.5, 0.0, 0.25) - 0.5).abs() < 1e-6);
        state.cue_blend = 1.0;
        assert!((state.cue_mix(0.5, 0.0, 0.25) - 0.25).abs() < 1e-6);
        state.cue_blend = 0.0;
        let expected = 0.75 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((state.cue_mix(0.5, 0.0, 0.25) - expected).abs() < 1e-6);
    }
}
