use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, ErrorKind, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use rtrb::{Consumer, Producer, PushError, RingBuffer};

use crate::media::{
    decode::{MediaDecoder, PcmChunk},
    resample::EngineRateDecoder,
};
use crate::tempo::{TempoProcessor, TempoSettings};

pub(crate) const CONTROL_QUEUE_CAPACITY: usize = 64;
pub(crate) const AUDIO_QUEUE_CAPACITY: usize = 16;
pub(crate) const RECYCLE_QUEUE_CAPACITY: usize = AUDIO_QUEUE_CAPACITY * 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RenderCommand {
    Play,
    Pause,
    SetGain(f32),
    Reset { generation: u64, playing: bool },
}

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Seek {
        seconds: f64,
        generation: u64,
    },
    Replace {
        path: PathBuf,
        generation: u64,
        tempo_settings: TempoSettings,
    },
    SetTempo(TempoSettings),
    Shutdown,
}

#[derive(Debug)]
pub(crate) struct DecodedBlock {
    pub(crate) generation: u64,
    pub(crate) chunk: PcmChunk,
    pub(crate) source_frames_per_output: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckMediaInfo {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub output_sample_rate: u32,
    pub channels: usize,
    pub duration_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeckState {
    Paused,
    Playing,
    Ended,
}

pub struct DeckTransport {
    control: Producer<RenderCommand>,
    worker_control: mpsc::Sender<WorkerCommand>,
    _stream: Stream,
    worker: Option<JoinHandle<()>>,
    metrics: Arc<DeckMetrics>,
    worker_error: Arc<Mutex<Option<String>>>,
    generation: u64,
    media: DeckMediaInfo,
}

impl DeckTransport {
    pub fn open_default(path: impl AsRef<Path>, initial_gain: f32) -> Result<Self, Box<dyn Error>> {
        let path = path.as_ref().to_path_buf();
        let decoder = MediaDecoder::open(&path)?;
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no default output device")?;
        let supported = device.default_output_config()?;
        let sample_format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let media = DeckMediaInfo {
            path: path.clone(),
            sample_rate: decoder.info().sample_rate,
            output_sample_rate: config.sample_rate,
            channels: decoder.info().channels,
            duration_seconds: decoder.info().duration_seconds,
        };
        let decoder = EngineRateDecoder::new(decoder, config.sample_rate)?;
        let (control, render_commands) = RingBuffer::new(CONTROL_QUEUE_CAPACITY);
        let (audio_blocks, render_blocks) = RingBuffer::new(AUDIO_QUEUE_CAPACITY);
        let (recycle_buffers, recycled_buffers) = RingBuffer::new(RECYCLE_QUEUE_CAPACITY);
        let (worker_control, worker_commands) = mpsc::channel();
        let metrics = Arc::new(DeckMetrics::new(1));
        let worker_error = Arc::new(Mutex::new(None));

        let worker = spawn_decoder_worker(
            decoder,
            1,
            audio_blocks,
            recycled_buffers,
            worker_commands,
            Arc::clone(&metrics),
            Arc::clone(&worker_error),
            TempoSettings::default(),
        );

        let stream = match sample_format {
            SampleFormat::F32 => build_deck_stream::<f32>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::F64 => build_deck_stream::<f64>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::I8 => build_deck_stream::<i8>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::I16 => build_deck_stream::<i16>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::I24 => build_deck_stream::<cpal::I24>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::I32 => build_deck_stream::<i32>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::I64 => build_deck_stream::<i64>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::U8 => build_deck_stream::<u8>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::U16 => build_deck_stream::<u16>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::U32 => build_deck_stream::<u32>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            SampleFormat::U64 => build_deck_stream::<u64>(
                &device,
                &config,
                render_commands,
                render_blocks,
                recycle_buffers,
                initial_gain,
                &metrics,
            )?,
            other => return Err(format!("unsupported output sample format: {other:?}").into()),
        };

        stream.play()?;
        Ok(Self {
            control,
            worker_control,
            _stream: stream,
            worker: Some(worker),
            metrics,
            worker_error,
            generation: 1,
            media,
        })
    }

    pub fn media(&self) -> &DeckMediaInfo {
        &self.media
    }

    pub fn play(&mut self) -> Result<(), DeckControlError> {
        self.send_render(RenderCommand::Play)
    }

    pub fn pause(&mut self) -> Result<(), DeckControlError> {
        self.send_render(RenderCommand::Pause)
    }

    pub fn set_gain(&mut self, gain: f32) -> Result<(), DeckControlError> {
        self.send_render(RenderCommand::SetGain(gain))
    }

    pub fn seek(&mut self, seconds: f64, resume: bool) -> Result<(), DeckControlError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(DeckControlError::InvalidSeek);
        }
        self.generation += 1;
        self.metrics
            .active_generation
            .store(self.generation, Ordering::Release);
        self.send_render(RenderCommand::Reset {
            generation: self.generation,
            playing: resume,
        })?;
        self.worker_control
            .send(WorkerCommand::Seek {
                seconds,
                generation: self.generation,
            })
            .map_err(|_| DeckControlError::WorkerStopped)
    }

    pub fn stop(&mut self) -> Result<(), DeckControlError> {
        self.seek(0.0, false)
    }

    pub fn replace_track(
        &mut self,
        path: impl AsRef<Path>,
        autoplay: bool,
    ) -> Result<(), Box<dyn Error>> {
        let path = path.as_ref().to_path_buf();
        let decoder = MediaDecoder::open(&path)?;
        self.media = DeckMediaInfo {
            path: path.clone(),
            sample_rate: decoder.info().sample_rate,
            output_sample_rate: self.media.output_sample_rate,
            channels: decoder.info().channels,
            duration_seconds: decoder.info().duration_seconds,
        };
        drop(decoder);

        self.generation += 1;
        self.metrics
            .active_generation
            .store(self.generation, Ordering::Release);
        self.send_render(RenderCommand::Reset {
            generation: self.generation,
            playing: autoplay,
        })?;
        self.worker_control.send(WorkerCommand::Replace {
            path,
            generation: self.generation,
            tempo_settings: TempoSettings::default(),
        })?;
        Ok(())
    }

    pub fn snapshot(&self) -> DeckSnapshot {
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
            position_frames: self.metrics.position_frames.load(Ordering::Relaxed),
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
            tempo_percent: f32::from_bits(self.metrics.tempo_percent_bits.load(Ordering::Relaxed)),
            key_lock: self.metrics.key_lock.load(Ordering::Relaxed),
            pitch_semitones: f32::from_bits(
                self.metrics.pitch_semitones_bits.load(Ordering::Relaxed),
            ),
            tempo_ratio: f64::from_bits(self.metrics.tempo_ratio_bits.load(Ordering::Relaxed)),
            processor_latency_frames: self
                .metrics
                .processor_latency_frames
                .load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(mut self) -> DeckSnapshot {
        let _ = self.worker_control.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.snapshot()
    }

    fn send_render(&mut self, command: RenderCommand) -> Result<(), DeckControlError> {
        self.control
            .push(command)
            .map_err(|_| DeckControlError::QueueFull)
    }
}

impl Drop for DeckTransport {
    fn drop(&mut self) {
        let _ = self.worker_control.send(WorkerCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeckSnapshot {
    pub state: DeckState,
    pub position_frames: u64,
    pub rendered_frames: u64,
    pub callbacks: u64,
    pub underflow_callbacks: u64,
    pub stale_blocks: u64,
    pub recycle_failures: u64,
    pub stream_errors: u64,
    pub generation: u64,
    pub worker_error: Option<String>,
    pub tempo_percent: f32,
    pub key_lock: bool,
    pub pitch_semitones: f32,
    pub tempo_ratio: f64,
    pub processor_latency_frames: u64,
}

impl DeckSnapshot {
    pub fn summary(&self) -> String {
        format!(
            "Deck report: state={:?}, generation={}, position_frames={}, rendered_frames={}, tempo_percent={}, key_lock={}, pitch_semitones={}, processor_latency_frames={}, callbacks={}, underflow_callbacks={}, stale_blocks={}, recycle_failures={}, stream_errors={}, worker_error={}",
            self.state,
            self.generation,
            self.position_frames,
            self.rendered_frames,
            self.tempo_percent,
            self.key_lock,
            self.pitch_semitones,
            self.processor_latency_frames,
            self.callbacks,
            self.underflow_callbacks,
            self.stale_blocks,
            self.recycle_failures,
            self.stream_errors,
            self.worker_error.as_deref().unwrap_or("none")
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeckControlError {
    QueueFull,
    WorkerStopped,
    InvalidSeek,
}

impl fmt::Display for DeckControlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QueueFull => formatter.write_str("deck control queue is full"),
            Self::WorkerStopped => formatter.write_str("decoder worker has stopped"),
            Self::InvalidSeek => formatter.write_str("seek must be finite and non-negative"),
        }
    }
}

impl Error for DeckControlError {}

pub(crate) struct DeckMetrics {
    pub(crate) active_generation: AtomicU64,
    pub(crate) decoded_eof_generation: AtomicU64,
    pub(crate) ready_generation: AtomicU64,
    pub(crate) ended_generation: AtomicU64,
    pub(crate) position_frames: AtomicU64,
    pub(crate) rendered_frames: AtomicU64,
    pub(crate) callbacks: AtomicU64,
    pub(crate) underflows: AtomicU64,
    pub(crate) stale_blocks: AtomicU64,
    pub(crate) recycle_failures: AtomicU64,
    pub(crate) stream_errors: AtomicU64,
    pub(crate) playing: AtomicBool,
    pub(crate) tempo_percent_bits: AtomicU32,
    pub(crate) key_lock: AtomicBool,
    pub(crate) pitch_semitones_bits: AtomicU32,
    pub(crate) tempo_ratio_bits: AtomicU64,
    pub(crate) processor_latency_frames: AtomicU64,
}

impl DeckMetrics {
    pub(crate) fn new(generation: u64) -> Self {
        Self {
            active_generation: AtomicU64::new(generation),
            decoded_eof_generation: AtomicU64::new(0),
            ready_generation: AtomicU64::new(0),
            ended_generation: AtomicU64::new(0),
            position_frames: AtomicU64::new(0),
            rendered_frames: AtomicU64::new(0),
            callbacks: AtomicU64::new(0),
            underflows: AtomicU64::new(0),
            stale_blocks: AtomicU64::new(0),
            recycle_failures: AtomicU64::new(0),
            stream_errors: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            tempo_percent_bits: AtomicU32::new(0.0_f32.to_bits()),
            key_lock: AtomicBool::new(false),
            pitch_semitones_bits: AtomicU32::new(0.0_f32.to_bits()),
            tempo_ratio_bits: AtomicU64::new(1.0_f64.to_bits()),
            processor_latency_frames: AtomicU64::new(0),
        }
    }
}

pub(crate) struct RenderState {
    generation: u64,
    playing: bool,
    gain: f32,
    current: Option<DecodedBlock>,
    frame_offset: usize,
    source_position: f64,
}

impl RenderState {
    pub(crate) fn new(gain: f32) -> Self {
        Self {
            generation: 1,
            playing: false,
            gain: gain.clamp(0.0, 1.0),
            current: None,
            frame_offset: 0,
            source_position: 0.0,
        }
    }

    pub(crate) fn apply_commands(
        &mut self,
        commands: &mut Consumer<RenderCommand>,
        recycle: &mut Producer<Vec<f32>>,
        metrics: &DeckMetrics,
    ) {
        while let Ok(command) = commands.pop() {
            match command {
                RenderCommand::Play => self.playing = true,
                RenderCommand::Pause => self.playing = false,
                RenderCommand::SetGain(gain) => self.gain = gain.clamp(0.0, 1.0),
                RenderCommand::Reset {
                    generation,
                    playing,
                } => {
                    self.recycle_current(recycle, metrics);
                    self.generation = generation;
                    self.playing = playing;
                    self.frame_offset = 0;
                    self.source_position = 0.0;
                    metrics.position_frames.store(0, Ordering::Relaxed);
                    metrics.ended_generation.store(0, Ordering::Release);
                }
            }
        }
        metrics.playing.store(self.playing, Ordering::Release);
    }

    pub(crate) fn render_frame(
        &mut self,
        output: &mut [f32],
        blocks: &mut Consumer<DecodedBlock>,
        recycle: &mut Producer<Vec<f32>>,
        metrics: &DeckMetrics,
    ) -> bool {
        output.fill(0.0);
        if !self.playing {
            return true;
        }

        loop {
            if self.current.is_none() {
                match blocks.pop() {
                    Ok(block) if block.generation == self.generation => {
                        self.current = Some(block);
                        self.frame_offset = 0;
                    }
                    Ok(block) => {
                        metrics.stale_blocks.fetch_add(1, Ordering::Relaxed);
                        recycle_buffer(block.chunk.samples, recycle, metrics);
                        continue;
                    }
                    Err(_) => {
                        if metrics.decoded_eof_generation.load(Ordering::Acquire) == self.generation
                        {
                            self.playing = false;
                            metrics.playing.store(false, Ordering::Release);
                            metrics
                                .ended_generation
                                .store(self.generation, Ordering::Release);
                            return true;
                        }
                        if metrics.ready_generation.load(Ordering::Acquire) != self.generation {
                            return true;
                        }
                        return false;
                    }
                }
            }

            let block = self.current.as_ref().expect("current block is set");
            if self.frame_offset >= block.chunk.frames() {
                self.recycle_current(recycle, metrics);
                continue;
            }

            let input_channels = block.chunk.channels;
            let base = self.frame_offset * input_channels;
            for (channel, sample) in output.iter_mut().enumerate() {
                let source_channel = if input_channels == 1 {
                    0
                } else {
                    channel.min(input_channels - 1)
                };
                *sample = block.chunk.samples[base + source_channel] * self.gain;
            }
            self.frame_offset += 1;
            self.source_position += block.source_frames_per_output;
            metrics
                .position_frames
                .store(self.source_position.round() as u64, Ordering::Relaxed);
            metrics.rendered_frames.fetch_add(1, Ordering::Relaxed);
            return true;
        }
    }

    fn recycle_current(&mut self, recycle: &mut Producer<Vec<f32>>, metrics: &DeckMetrics) {
        if let Some(block) = self.current.take() {
            recycle_buffer(block.chunk.samples, recycle, metrics);
        }
    }
}

fn recycle_buffer(mut buffer: Vec<f32>, recycle: &mut Producer<Vec<f32>>, metrics: &DeckMetrics) {
    buffer.clear();
    if let Err(buffer) = recycle.push(buffer) {
        metrics.recycle_failures.fetch_add(1, Ordering::Relaxed);
        // Avoid running a Vec destructor on the real-time callback if an invariant is violated.
        std::mem::forget(buffer);
    }
}

fn build_deck_stream<T>(
    device: &Device,
    config: &StreamConfig,
    mut commands: Consumer<RenderCommand>,
    mut blocks: Consumer<DecodedBlock>,
    mut recycle: Producer<Vec<f32>>,
    initial_gain: f32,
    metrics: &Arc<DeckMetrics>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let callback_metrics = Arc::clone(metrics);
    let error_metrics = Arc::clone(metrics);
    let output_channels = usize::from(config.channels);
    let mut state = RenderState::new(initial_gain);
    let mut frame = [0.0_f32; 32];
    if output_channels > frame.len() {
        return Err(cpal::Error::with_message(
            ErrorKind::UnsupportedConfig,
            "more than 32 output channels",
        ));
    }

    device.build_output_stream(
        *config,
        move |output: &mut [T], _| {
            callback_metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            state.apply_commands(&mut commands, &mut recycle, &callback_metrics);
            let mut underflow = false;
            for output_frame in output.chunks_mut(output_channels) {
                if !state.render_frame(
                    &mut frame[..output_channels],
                    &mut blocks,
                    &mut recycle,
                    &callback_metrics,
                ) {
                    underflow = true;
                }
                for (sample, value) in output_frame.iter_mut().zip(frame.iter()) {
                    *sample = T::from_sample(*value);
                }
            }
            if underflow {
                callback_metrics.underflows.fetch_add(1, Ordering::Relaxed);
            }
        },
        move |_error| {
            error_metrics.stream_errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_decoder_worker(
    decoder: EngineRateDecoder,
    generation: u64,
    mut output: Producer<DecodedBlock>,
    mut recycled: Consumer<Vec<f32>>,
    commands: Receiver<WorkerCommand>,
    metrics: Arc<DeckMetrics>,
    worker_error: Arc<Mutex<Option<String>>>,
    initial_tempo_settings: TempoSettings,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut decoder = decoder;
        let mut tempo = match TempoProcessor::new(
            decoder.target_rate(),
            decoder.info().channels,
            initial_tempo_settings,
        ) {
            Ok(processor) => processor,
            Err(error) => {
                store_worker_error(&worker_error, error.to_string());
                return;
            }
        };
        metrics
            .processor_latency_frames
            .store(tempo.latency_frames() as u64, Ordering::Relaxed);
        let mut generation = generation;
        let mut pending: Option<DecodedBlock> = None;
        let mut eof = false;
        let mut eof_reported = false;
        let mut decode_buffer = Vec::new();

        loop {
            match commands.try_recv() {
                Ok(WorkerCommand::Seek {
                    seconds,
                    generation: next_generation,
                }) => match decoder.seek(seconds) {
                    Ok(_) => {
                        tempo.reset();
                        generation = next_generation;
                        pending = None;
                        eof = false;
                        eof_reported = false;
                        decode_buffer.clear();
                        metrics.decoded_eof_generation.store(0, Ordering::Release);
                        metrics.ready_generation.store(0, Ordering::Release);
                    }
                    Err(error) => {
                        store_worker_error(&worker_error, error.to_string());
                        return;
                    }
                },
                Ok(WorkerCommand::Replace {
                    path,
                    generation: next_generation,
                    tempo_settings,
                }) => match MediaDecoder::open(path)
                    .and_then(|source| EngineRateDecoder::new(source, decoder.target_rate()))
                {
                    Ok(next_decoder) => {
                        decoder = next_decoder;
                        tempo = match TempoProcessor::new(
                            decoder.target_rate(),
                            decoder.info().channels,
                            tempo_settings,
                        ) {
                            Ok(processor) => processor,
                            Err(error) => {
                                store_worker_error(&worker_error, error.to_string());
                                return;
                            }
                        };
                        metrics
                            .processor_latency_frames
                            .store(tempo.latency_frames() as u64, Ordering::Relaxed);
                        generation = next_generation;
                        pending = None;
                        eof = false;
                        eof_reported = false;
                        decode_buffer.clear();
                        metrics.decoded_eof_generation.store(0, Ordering::Release);
                        metrics.ready_generation.store(0, Ordering::Release);
                    }
                    Err(error) => {
                        store_worker_error(&worker_error, error.to_string());
                        return;
                    }
                },
                Ok(WorkerCommand::SetTempo(settings)) => {
                    if let Err(error) = tempo.set_settings(settings) {
                        store_worker_error(&worker_error, error.to_string());
                        return;
                    }
                    metrics
                        .tempo_percent_bits
                        .store(settings.tempo_percent.to_bits(), Ordering::Relaxed);
                    metrics.key_lock.store(settings.key_lock, Ordering::Relaxed);
                    metrics
                        .pitch_semitones_bits
                        .store(settings.pitch_semitones.to_bits(), Ordering::Relaxed);
                    metrics
                        .tempo_ratio_bits
                        .store(settings.tempo_ratio().to_bits(), Ordering::Relaxed);
                    metrics
                        .processor_latency_frames
                        .store(tempo.latency_frames() as u64, Ordering::Relaxed);
                }
                Ok(WorkerCommand::Shutdown) => return,
                Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => {}
            }

            if let Some(block) = pending.take() {
                let block_generation = block.generation;
                match output.push(block) {
                    Ok(()) => {
                        metrics
                            .ready_generation
                            .store(block_generation, Ordering::Release);
                        continue;
                    }
                    Err(PushError::Full(block)) => {
                        pending = Some(block);
                        thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                }
            }

            if eof {
                if !eof_reported {
                    metrics
                        .decoded_eof_generation
                        .store(generation, Ordering::Release);
                    eof_reported = true;
                }
                thread::sleep(Duration::from_millis(2));
                continue;
            }

            match decoder.next_chunk_into(std::mem::take(&mut decode_buffer)) {
                Ok(Some(chunk)) => {
                    let ratio = tempo.settings().tempo_ratio();
                    match tempo.process(chunk, recycled.pop().unwrap_or_default()) {
                        Ok((processed, input_buffer)) => {
                            decode_buffer = input_buffer;
                            if let Some(chunk) = processed {
                                pending = Some(DecodedBlock {
                                    generation,
                                    chunk,
                                    source_frames_per_output: ratio,
                                });
                            }
                        }
                        Err(error) => {
                            store_worker_error(&worker_error, error.to_string());
                            return;
                        }
                    }
                }
                Ok(None) => {
                    let ratio = tempo.settings().tempo_ratio();
                    if let Some(chunk) = tempo.flush(recycled.pop().unwrap_or_default()) {
                        pending = Some(DecodedBlock {
                            generation,
                            chunk,
                            source_frames_per_output: ratio,
                        });
                    }
                    eof = true;
                }
                Err(error) => {
                    store_worker_error(&worker_error, error.to_string());
                    return;
                }
            }
        }
    })
}

pub(crate) fn store_worker_error(target: &Mutex<Option<String>>, error: String) {
    if let Ok(mut value) = target.lock() {
        *value = Some(error);
    }
}

pub fn wait_until_ended(deck: &DeckTransport, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if deck.snapshot().state == DeckState::Ended {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(generation: u64, frames: &[[f32; 2]]) -> DecodedBlock {
        DecodedBlock {
            generation,
            chunk: PcmChunk {
                samples: frames.iter().flatten().copied().collect(),
                sample_rate: 44_100,
                channels: 2,
            },
            source_frames_per_output: 1.0,
        }
    }

    #[test]
    fn render_state_plays_pauses_and_applies_gain() {
        let (mut command_tx, mut command_rx) = RingBuffer::new(8);
        let (mut block_tx, mut block_rx) = RingBuffer::new(8);
        let (mut recycle_tx, _recycle_rx) = RingBuffer::new(8);
        let metrics = DeckMetrics::new(1);
        let mut state = RenderState::new(0.5);
        block_tx
            .push(block(1, &[[0.4, -0.4], [0.2, -0.2]]))
            .unwrap();

        command_tx.push(RenderCommand::Play).unwrap();
        state.apply_commands(&mut command_rx, &mut recycle_tx, &metrics);
        let mut output = [0.0; 2];
        assert!(state.render_frame(&mut output, &mut block_rx, &mut recycle_tx, &metrics));
        assert_eq!(output, [0.2, -0.2]);

        command_tx.push(RenderCommand::Pause).unwrap();
        state.apply_commands(&mut command_rx, &mut recycle_tx, &metrics);
        assert!(state.render_frame(&mut output, &mut block_rx, &mut recycle_tx, &metrics));
        assert_eq!(output, [0.0, 0.0]);
        assert_eq!(metrics.position_frames.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn generation_reset_discards_stale_audio() {
        let (mut command_tx, mut command_rx) = RingBuffer::new(8);
        let (mut block_tx, mut block_rx) = RingBuffer::new(8);
        let (mut recycle_tx, mut recycle_rx) = RingBuffer::new(8);
        let metrics = DeckMetrics::new(2);
        let mut state = RenderState::new(1.0);
        block_tx.push(block(1, &[[0.9, 0.9]])).unwrap();
        block_tx.push(block(2, &[[0.25, -0.25]])).unwrap();
        command_tx
            .push(RenderCommand::Reset {
                generation: 2,
                playing: true,
            })
            .unwrap();
        state.apply_commands(&mut command_rx, &mut recycle_tx, &metrics);

        let mut output = [0.0; 2];
        assert!(state.render_frame(&mut output, &mut block_rx, &mut recycle_tx, &metrics));
        assert_eq!(output, [0.25, -0.25]);
        assert_eq!(metrics.stale_blocks.load(Ordering::Relaxed), 1);
        assert!(recycle_rx.pop().is_ok());
    }

    #[test]
    fn eof_changes_playing_state_to_ended() {
        let (_command_tx, _command_rx) = RingBuffer::<RenderCommand>::new(1);
        let (_block_tx, mut block_rx) = RingBuffer::<DecodedBlock>::new(1);
        let (mut recycle_tx, _recycle_rx) = RingBuffer::new(1);
        let metrics = DeckMetrics::new(1);
        metrics.decoded_eof_generation.store(1, Ordering::Release);
        let mut state = RenderState::new(1.0);
        state.playing = true;

        let mut output = [1.0; 2];
        assert!(state.render_frame(&mut output, &mut block_rx, &mut recycle_tx, &metrics));
        assert_eq!(output, [0.0, 0.0]);
        assert!(!state.playing);
        assert_eq!(metrics.ended_generation.load(Ordering::Acquire), 1);
    }

    #[test]
    fn decoder_worker_supports_live_tempo_seek_and_track_replacement() {
        let decoder = EngineRateDecoder::new(
            MediaDecoder::open("tests/fixtures/audio/tone.wav").unwrap(),
            44_100,
        )
        .unwrap();
        let (output_tx, mut output_rx) = RingBuffer::new(16);
        let (_recycle_tx, recycle_rx) = RingBuffer::new(32);
        let (command_tx, command_rx) = mpsc::channel();
        let metrics = Arc::new(DeckMetrics::new(1));
        let error = Arc::new(Mutex::new(None));
        let worker = spawn_decoder_worker(
            decoder,
            1,
            output_tx,
            recycle_rx,
            command_rx,
            Arc::clone(&metrics),
            Arc::clone(&error),
            TempoSettings::default(),
        );

        let first = wait_for_block(&mut output_rx, 1);
        assert_eq!(first.generation, 1);
        command_tx
            .send(WorkerCommand::SetTempo(TempoSettings {
                tempo_percent: 8.0,
                ..TempoSettings::default()
            }))
            .unwrap();
        let after_tempo = wait_for_block(&mut output_rx, 1);
        assert_eq!(after_tempo.generation, 1);
        command_tx
            .send(WorkerCommand::Seek {
                seconds: 1.5,
                generation: 2,
            })
            .unwrap();
        let after_seek = wait_for_block(&mut output_rx, 2);
        assert_eq!(after_seek.generation, 2);
        command_tx
            .send(WorkerCommand::Replace {
                path: PathBuf::from("tests/fixtures/audio/tone.flac"),
                generation: 3,
                tempo_settings: TempoSettings::default(),
            })
            .unwrap();
        let replacement = wait_for_block(&mut output_rx, 3);
        assert_eq!(replacement.generation, 3);

        command_tx.send(WorkerCommand::Shutdown).unwrap();
        worker.join().unwrap();
        assert_eq!(*error.lock().unwrap(), None);
    }

    fn wait_for_block(consumer: &mut Consumer<DecodedBlock>, generation: u64) -> DecodedBlock {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(block) = consumer.pop() {
                if block.generation == generation {
                    return block;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for generation"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
}
