use std::{
    path::PathBuf,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    thread::{self, JoinHandle},
};

use djapp_audio_spike::{
    deck::{DeckSnapshot, DeckState},
    mixer::{DeckId, MixerEngine},
    tempo::TempoSettings,
};

#[derive(Clone, Debug)]
pub struct DeckServiceSnapshot {
    pub loaded_track_id: Option<i64>,
    pub title: Option<String>,
    pub path: Option<String>,
    pub duration_seconds: Option<f64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<usize>,
    pub state: DeckState,
    pub position_frames: u64,
    pub callbacks: u64,
    pub rendered_frames: u64,
    pub underflow_callbacks: u64,
    pub stale_blocks: u64,
    pub recycle_failures: u64,
    pub stream_errors: u64,
    pub worker_error: Option<String>,
    pub tempo_percent: f32,
    pub key_lock: bool,
    pub pitch_semitones: f32,
    pub tempo_ratio: f64,
    pub processor_latency_frames: u64,
}

#[derive(Clone, Debug)]
pub struct MixerServiceSnapshot {
    pub deck_a: DeckServiceSnapshot,
    pub deck_b: DeckServiceSnapshot,
    pub crossfader: f32,
    pub channel_gain_a: f32,
    pub channel_gain_b: f32,
    pub master_gain: f32,
    pub callbacks: u64,
    pub rendered_frames: u64,
    pub clipped_samples: u64,
    pub stream_errors: u64,
    pub output_device_id: Option<String>,
    pub output_device_name: Option<String>,
    pub device_recoveries: u64,
    pub device_message: Option<String>,
    pub cue_a: bool,
    pub cue_b: bool,
    pub cue_blend: f32,
    pub cue_gain: f32,
    pub cue_supported: bool,
    pub routing_mode: String,
    pub routing_preference: String,
    pub routing_limitation: Option<String>,
    pub cue_output_device_id: Option<String>,
    pub cue_output_device_name: Option<String>,
    pub cue_delay_ms: u32,
    pub cue_callbacks: u64,
    pub cue_rendered_frames: u64,
    pub cue_queue_depth_frames: u64,
    pub cue_min_queue_depth_frames: u64,
    pub cue_max_queue_depth_frames: u64,
    pub cue_underflow_callbacks: u64,
    pub cue_overflow_callbacks: u64,
    pub cue_stream_errors: u64,
    pub cue_signal_peak: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CuePreferences {
    pub cue_a: bool,
    pub cue_b: bool,
    pub blend: f32,
    pub gain: f32,
}

impl Default for CuePreferences {
    fn default() -> Self {
        Self {
            cue_a: false,
            cue_b: false,
            blend: -1.0,
            gain: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingMode {
    Automatic,
    MasterOnly,
    DualDeviceCue,
}

impl RoutingMode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "automatic" => Ok(Self::Automatic),
            "master-only" => Ok(Self::MasterOnly),
            "dual-device-cue" => Ok(Self::DualDeviceCue),
            _ => Err("unknown audio routing mode".to_string()),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::MasterOnly => "master-only",
            Self::DualDeviceCue => "dual-device-cue",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoutingPreferences {
    pub mode: RoutingMode,
    pub cue_output_device_id: Option<String>,
    pub cue_delay_ms: u32,
}

impl Default for RoutingPreferences {
    fn default() -> Self {
        Self {
            mode: RoutingMode::Automatic,
            cue_output_device_id: None,
            cue_delay_ms: 0,
        }
    }
}

#[derive(Clone)]
struct LoadedTrack {
    id: i64,
    title: String,
    path: PathBuf,
}

type Response = Sender<Result<MixerServiceSnapshot, String>>;

enum Command {
    Load {
        deck: DeckId,
        track_id: i64,
        title: String,
        path: PathBuf,
        response: Response,
    },
    Play {
        deck: DeckId,
        response: Response,
    },
    Pause {
        deck: DeckId,
        response: Response,
    },
    Seek {
        deck: DeckId,
        seconds: f64,
        resume: bool,
        response: Response,
    },
    Stop {
        deck: DeckId,
        response: Response,
    },
    SetChannelGain {
        deck: DeckId,
        gain: f32,
        response: Response,
    },
    SetTempo {
        deck: DeckId,
        percent: f32,
        response: Response,
    },
    SetKeyLock {
        deck: DeckId,
        enabled: bool,
        response: Response,
    },
    SetPitch {
        deck: DeckId,
        semitones: f32,
        response: Response,
    },
    SetCrossfader {
        value: f32,
        response: Response,
    },
    SetMasterGain {
        gain: f32,
        response: Response,
    },
    SetCue {
        deck: DeckId,
        enabled: bool,
        response: Response,
    },
    SetCueBlend {
        value: f32,
        response: Response,
    },
    SetCueGain {
        gain: f32,
        response: Response,
    },
    SelectOutputDevice {
        device_id: String,
        response: Response,
    },
    SelectCueOutputDevice {
        device_id: String,
        response: Response,
    },
    SetRoutingMode {
        mode: RoutingMode,
        response: Response,
    },
    SetCueDelay {
        delay_ms: u32,
        response: Response,
    },
    Snapshot {
        response: Response,
    },
    Shutdown,
}

pub struct MixerService {
    sender: Sender<Command>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl MixerService {
    pub fn start(
        preferred_output_device: Option<String>,
        cue_preferences: CuePreferences,
        routing_preferences: RoutingPreferences,
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let join = thread::Builder::new()
            .name("djapp-mixer-service".to_string())
            .spawn(move || {
                run(
                    receiver,
                    preferred_output_device,
                    cue_preferences,
                    routing_preferences,
                )
            })
            .map_err(|error| format!("failed to start mixer service: {error}"))?;
        Ok(Self {
            sender,
            join: Mutex::new(Some(join)),
        })
    }

    pub fn load(
        &self,
        deck: DeckId,
        track_id: i64,
        title: String,
        path: PathBuf,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Load {
            deck,
            track_id,
            title,
            path,
            response,
        })
    }

    pub fn play(&self, deck: DeckId) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Play { deck, response })
    }

    pub fn pause(&self, deck: DeckId) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Pause { deck, response })
    }

    pub fn seek(
        &self,
        deck: DeckId,
        seconds: f64,
        resume: bool,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Seek {
            deck,
            seconds,
            resume,
            response,
        })
    }

    pub fn stop(&self, deck: DeckId) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Stop { deck, response })
    }

    pub fn set_channel_gain(
        &self,
        deck: DeckId,
        gain: f32,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetChannelGain {
            deck,
            gain,
            response,
        })
    }

    pub fn set_tempo(
        &self,
        deck: DeckId,
        percent: f32,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetTempo {
            deck,
            percent,
            response,
        })
    }

    pub fn set_key_lock(
        &self,
        deck: DeckId,
        enabled: bool,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetKeyLock {
            deck,
            enabled,
            response,
        })
    }

    pub fn set_pitch(
        &self,
        deck: DeckId,
        semitones: f32,
    ) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetPitch {
            deck,
            semitones,
            response,
        })
    }

    pub fn set_crossfader(&self, value: f32) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetCrossfader { value, response })
    }

    pub fn set_master_gain(&self, gain: f32) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetMasterGain { gain, response })
    }

    pub fn snapshot(&self) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::Snapshot { response })
    }

    pub fn select_output_device(&self, device_id: String) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SelectOutputDevice {
            device_id,
            response,
        })
    }

    pub fn select_cue_output_device(
        &self,
        device_id: String,
    ) -> Result<MixerServiceSnapshot, String> {
        if device_id.is_empty() {
            return Err("select a headphone cue output".to_string());
        }
        self.request(|response| Command::SelectCueOutputDevice {
            device_id,
            response,
        })
    }

    pub fn set_routing_mode(&self, mode: RoutingMode) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetRoutingMode { mode, response })
    }

    pub fn set_cue_delay(&self, delay_ms: u32) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetCueDelay { delay_ms, response })
    }

    pub fn set_cue(&self, deck: DeckId, enabled: bool) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetCue {
            deck,
            enabled,
            response,
        })
    }

    pub fn set_cue_blend(&self, value: f32) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetCueBlend { value, response })
    }

    pub fn set_cue_gain(&self, gain: f32) -> Result<MixerServiceSnapshot, String> {
        self.request(|response| Command::SetCueGain { gain, response })
    }

    fn request(
        &self,
        build: impl FnOnce(Response) -> Command,
    ) -> Result<MixerServiceSnapshot, String> {
        let (response, receiver) = mpsc::channel();
        self.sender
            .send(build(response))
            .map_err(|_| "mixer service is unavailable".to_string())?;
        receiver
            .recv()
            .map_err(|_| "mixer service stopped before responding".to_string())?
    }
}

impl Drop for MixerService {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Ok(mut join) = self.join.lock() {
            if let Some(join) = join.take() {
                let _ = join.join();
            }
        }
    }
}

fn run(
    receiver: Receiver<Command>,
    preferred_output_device: Option<String>,
    mut cue: CuePreferences,
    mut routing: RoutingPreferences,
) {
    let mut engine: Option<MixerEngine> = None;
    let mut loaded_a: Option<LoadedTrack> = None;
    let mut loaded_b: Option<LoadedTrack> = None;
    let mut crossfader = 0.0;
    let mut channel_gain_a = 1.0;
    let mut channel_gain_b = 1.0;
    let mut master_gain = 1.0;
    let mut output_device_id = preferred_output_device;
    let mut output_device_name = device_name(output_device_id.as_deref());
    let mut device_recoveries = 0;
    let mut device_message = None;
    let mut observed_stream_errors = 0;

    while let Ok(command) = receiver.recv() {
        let result = match command {
            Command::Load {
                deck,
                track_id,
                title,
                path,
                response,
            } => {
                let result = (|| {
                    if engine.is_none() {
                        match open_engine(output_device_id.as_deref(), &routing) {
                            Ok(next) => engine = Some(next),
                            Err(preferred_error) if output_device_id.is_some() => {
                                let next = MixerEngine::open_default_unloaded()
                                    .map_err(|error| format!("{preferred_error}; default output also failed: {error}"))?;
                                let fallback = default_device();
                                output_device_id = fallback.as_ref().map(|device| device.0.clone());
                                output_device_name = fallback.map(|device| device.1);
                                device_recoveries += 1;
                                device_message = Some(format!(
                                    "Saved output was unavailable. Using {}.",
                                    output_device_name
                                        .as_deref()
                                        .unwrap_or("the default output")
                                ));
                                engine = Some(next);
                            }
                            Err(error) => return Err(error),
                        }
                        apply_automatic_cue(engine.as_mut().unwrap(), crossfader, &mut cue)?;
                    }
                    engine
                        .as_mut()
                        .unwrap()
                        .load_track(deck, &path, false)
                        .map_err(|error| error.to_string())?;
                    let loaded = LoadedTrack {
                        id: track_id,
                        title,
                        path,
                    };
                    match deck {
                        DeckId::A => loaded_a = Some(loaded),
                        DeckId::B => loaded_b = Some(loaded),
                    }
                    Ok(snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    ))
                })();
                let _ = response.send(result);
                continue;
            }
            Command::Play { deck, response } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.play(deck),
            ),
            Command::Pause { deck, response } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.pause(deck),
            ),
            Command::Seek {
                deck,
                seconds,
                resume,
                response,
            } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.seek(deck, seconds, resume),
            ),
            Command::Stop { deck, response } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.stop(deck),
            ),
            Command::SetTempo {
                deck,
                percent,
                response,
            } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.set_tempo(deck, percent),
            ),
            Command::SetKeyLock {
                deck,
                enabled,
                response,
            } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.set_key_lock(deck, enabled),
            ),
            Command::SetPitch {
                deck,
                semitones,
                response,
            } => respond(
                response,
                &mut engine,
                &loaded_a,
                &loaded_b,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &routing,
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.set_pitch(deck, semitones),
            ),
            Command::SetChannelGain {
                deck,
                gain,
                response,
            } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| {
                        engine
                            .set_channel_gain(deck, gain)
                            .map_err(|error| error.to_string())
                    });
                if result.is_ok() {
                    match deck {
                        DeckId::A => channel_gain_a = gain.clamp(0.0, 1.0),
                        DeckId::B => channel_gain_b = gain.clamp(0.0, 1.0),
                    }
                }
                let sent = response
                    .send(result.map(|()| {
                        snapshot(
                            engine.as_ref(),
                            loaded_a.as_ref(),
                            loaded_b.as_ref(),
                            crossfader,
                            channel_gain_a,
                            channel_gain_b,
                            master_gain,
                            cue,
                            &routing,
                            output_device_id.as_deref(),
                            output_device_name.as_deref(),
                            device_recoveries,
                            device_message.as_deref(),
                        )
                    }))
                    .is_ok();
                if !sent {
                    break;
                }
                continue;
            }
            Command::SetCrossfader { value, response } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| {
                        engine
                            .set_crossfader(value)
                            .map_err(|error| error.to_string())?;
                        apply_automatic_cue(engine, value, &mut cue)
                    });
                if result.is_ok() {
                    crossfader = value.clamp(-1.0, 1.0);
                }
                let sent = response
                    .send(result.map(|()| {
                        snapshot(
                            engine.as_ref(),
                            loaded_a.as_ref(),
                            loaded_b.as_ref(),
                            crossfader,
                            channel_gain_a,
                            channel_gain_b,
                            master_gain,
                            cue,
                            &routing,
                            output_device_id.as_deref(),
                            output_device_name.as_deref(),
                            device_recoveries,
                            device_message.as_deref(),
                        )
                    }))
                    .is_ok();
                if !sent {
                    break;
                }
                continue;
            }
            Command::SetMasterGain { gain, response } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| {
                        engine
                            .set_master_gain(gain)
                            .map_err(|error| error.to_string())
                    });
                if result.is_ok() {
                    master_gain = gain.clamp(0.0, 1.0);
                }
                let sent = response
                    .send(result.map(|()| {
                        snapshot(
                            engine.as_ref(),
                            loaded_a.as_ref(),
                            loaded_b.as_ref(),
                            crossfader,
                            channel_gain_a,
                            channel_gain_b,
                            master_gain,
                            cue,
                            &routing,
                            output_device_id.as_deref(),
                            output_device_name.as_deref(),
                            device_recoveries,
                            device_message.as_deref(),
                        )
                    }))
                    .is_ok();
                if !sent {
                    break;
                }
                continue;
            }
            Command::SetCue {
                deck,
                enabled,
                response,
            } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| engine.set_cue(deck, enabled).map_err(|e| e.to_string()));
                if result.is_ok() {
                    match deck {
                        DeckId::A => cue.cue_a = enabled,
                        DeckId::B => cue.cue_b = enabled,
                    }
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SetCueBlend { value, response } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| engine.set_cue_blend(value).map_err(|e| e.to_string()));
                if result.is_ok() {
                    cue.blend = value.clamp(-1.0, 1.0);
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SetCueGain { gain, response } => {
                let result = engine
                    .as_mut()
                    .ok_or_else(|| "no track is loaded in either deck".to_string())
                    .and_then(|engine| engine.set_cue_gain(gain).map_err(|e| e.to_string()));
                if result.is_ok() {
                    cue.gain = gain.clamp(0.0, 1.0);
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SelectOutputDevice {
                device_id,
                response,
            } => {
                let result = if engine.is_some() {
                    restart_engine(
                        &mut engine,
                        Some(&device_id),
                        &loaded_a,
                        &loaded_b,
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                    )
                } else {
                    open_engine(Some(&device_id), &routing).map(drop)
                };
                if result.is_ok() {
                    output_device_name = device_name(Some(&device_id));
                    output_device_id = Some(device_id);
                    device_message = Some(format!(
                        "Output changed to {}.",
                        output_device_name.as_deref().unwrap_or("selected device")
                    ));
                    observed_stream_errors = 0;
                } else if engine.is_some() {
                    let fallback = default_device();
                    output_device_id = fallback.as_ref().map(|device| device.0.clone());
                    output_device_name = fallback.map(|device| device.1);
                    device_recoveries += 1;
                    device_message = Some(
                        "Selected output could not be opened; playback was restored on the macOS default output."
                            .to_string(),
                    );
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SelectCueOutputDevice {
                device_id,
                response,
            } => {
                let mut candidate = routing.clone();
                candidate.cue_output_device_id = Some(device_id.clone());
                let result = if engine.is_some() && candidate.mode == RoutingMode::DualDeviceCue {
                    restart_engine(
                        &mut engine,
                        output_device_id.as_deref(),
                        &loaded_a,
                        &loaded_b,
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &candidate,
                    )
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    routing = candidate;
                    device_message = Some(format!(
                        "Cue output selected: {}.",
                        device_name(Some(&device_id)).as_deref().unwrap_or("selected device")
                    ));
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SetRoutingMode { mode, response } => {
                let mut candidate = routing.clone();
                candidate.mode = mode;
                let result = if engine.is_some() {
                    restart_engine(
                        &mut engine,
                        output_device_id.as_deref(),
                        &loaded_a,
                        &loaded_b,
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &candidate,
                    )
                } else if mode == RoutingMode::DualDeviceCue
                    && candidate.cue_output_device_id.is_none()
                {
                    Err("select a cue output before enabling dual-device cue".to_string())
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    routing = candidate;
                    device_message = Some(format!(
                        "Audio routing changed to {}.",
                        routing.mode.as_str()
                    ));
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::SetCueDelay { delay_ms, response } => {
                let delay_ms = delay_ms.min(250);
                let mut candidate = routing.clone();
                candidate.cue_delay_ms = delay_ms;
                let result = if engine.is_some() && candidate.mode == RoutingMode::DualDeviceCue {
                    restart_engine(
                        &mut engine,
                        output_device_id.as_deref(),
                        &loaded_a,
                        &loaded_b,
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &candidate,
                    )
                } else {
                    Ok(())
                };
                if result.is_ok() {
                    routing = candidate;
                }
                let _ = response.send(result.map(|()| {
                    snapshot(
                        engine.as_ref(),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::Snapshot { response } => {
                let cue_health = engine
                    .as_ref()
                    .and_then(|engine| engine.snapshot().dual_cue)
                    .map(|cue| {
                        (
                            cue.stream_errors,
                            cue.underflow_callbacks,
                            cue.overflow_callbacks,
                        )
                    })
                    .unwrap_or((0, 0, 0));
                if (cue_health.0 > 0 || cue_health.1 >= 3 || cue_health.2 >= 3)
                    && engine
                        .as_mut()
                        .map(MixerEngine::disable_dual_cue)
                        .unwrap_or(false)
                {
                    device_message = Some(if cue_health.0 > 0 {
                        "Headphone cue stopped after a cue-device error; master playback continues."
                            .to_string()
                    } else {
                        "Headphone cue stopped after repeated buffer instability; master playback continues."
                            .to_string()
                    });
                }
                let current_errors = engine
                    .as_ref()
                    .map(|engine| engine.snapshot().stream_errors)
                    .unwrap_or(0);
                if current_errors > observed_stream_errors {
                    let preferred = output_device_id.clone();
                    let recovery = restart_engine(
                        &mut engine,
                        preferred.as_deref(),
                        &loaded_a,
                        &loaded_b,
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        &routing,
                    );
                    match recovery {
                        Ok(()) => {
                            device_recoveries += 1;
                            let active = if preferred
                                .as_deref()
                                .and_then(|id| device_name(Some(id)))
                                .is_some()
                            {
                                preferred
                                    .and_then(|id| device_name(Some(&id)).map(|name| (id, name)))
                            } else {
                                default_device()
                            };
                            output_device_id = active.as_ref().map(|device| device.0.clone());
                            output_device_name = active.map(|device| device.1);
                            device_message =
                                Some("Audio output recovered after a device error.".to_string());
                            observed_stream_errors = 0;
                        }
                        Err(error) if engine.is_some() => {
                            device_recoveries += 1;
                            let fallback = default_device();
                            output_device_id = fallback.as_ref().map(|device| device.0.clone());
                            output_device_name = fallback.map(|device| device.1);
                            device_message = Some(error);
                            observed_stream_errors = 0;
                        }
                        Err(error) => device_message = Some(error),
                    }
                } else {
                    observed_stream_errors = current_errors;
                }
                let _ = response.send(Ok(snapshot(
                    engine.as_ref(),
                    loaded_a.as_ref(),
                    loaded_b.as_ref(),
                    crossfader,
                    channel_gain_a,
                    channel_gain_b,
                    master_gain,
                    cue,
                    &routing,
                    output_device_id.as_deref(),
                    output_device_name.as_deref(),
                    device_recoveries,
                    device_message.as_deref(),
                )));
                continue;
            }
            Command::Shutdown => break,
        };
        if !result {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn respond<E>(
    response: Response,
    engine: &mut Option<MixerEngine>,
    loaded_a: &Option<LoadedTrack>,
    loaded_b: &Option<LoadedTrack>,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    cue: CuePreferences,
    routing: &RoutingPreferences,
    output_device_id: Option<&str>,
    output_device_name: Option<&str>,
    device_recoveries: u64,
    device_message: Option<&str>,
    operation: impl FnOnce(&mut MixerEngine) -> Result<(), E>,
) -> bool
where
    E: std::fmt::Display,
{
    let result = engine
        .as_mut()
        .ok_or_else(|| "no track is loaded in either deck".to_string())
        .and_then(|engine| {
            operation(engine)
                .map_err(|error| error.to_string())
                .map(|()| {
                    snapshot(
                        Some(engine),
                        loaded_a.as_ref(),
                        loaded_b.as_ref(),
                        crossfader,
                        channel_gain_a,
                        channel_gain_b,
                        master_gain,
                        cue,
                        routing,
                        output_device_id,
                        output_device_name,
                        device_recoveries,
                        device_message,
                    )
                })
        });
    response.send(result).is_ok()
}

fn unloaded() -> DeckServiceSnapshot {
    DeckServiceSnapshot {
        loaded_track_id: None,
        title: None,
        path: None,
        duration_seconds: None,
        sample_rate: None,
        channels: None,
        state: DeckState::Paused,
        position_frames: 0,
        callbacks: 0,
        rendered_frames: 0,
        underflow_callbacks: 0,
        stale_blocks: 0,
        recycle_failures: 0,
        stream_errors: 0,
        worker_error: None,
        tempo_percent: 0.0,
        key_lock: false,
        pitch_semitones: 0.0,
        tempo_ratio: 1.0,
        processor_latency_frames: 0,
    }
}

fn deck_snapshot(
    engine: &MixerEngine,
    deck: DeckId,
    loaded: Option<&LoadedTrack>,
) -> DeckServiceSnapshot {
    let Some(loaded) = loaded else {
        return unloaded();
    };
    let Some(media) = engine.media(deck) else {
        return unloaded();
    };
    let DeckSnapshot {
        state,
        position_frames,
        callbacks,
        rendered_frames,
        underflow_callbacks,
        stale_blocks,
        recycle_failures,
        stream_errors,
        worker_error,
        tempo_percent,
        key_lock,
        pitch_semitones,
        tempo_ratio,
        processor_latency_frames,
        ..
    } = match deck {
        DeckId::A => engine.snapshot().deck_a,
        DeckId::B => engine.snapshot().deck_b,
    };
    DeckServiceSnapshot {
        loaded_track_id: Some(loaded.id),
        title: Some(loaded.title.clone()),
        path: Some(loaded.path.to_string_lossy().into_owned()),
        duration_seconds: media.duration_seconds,
        sample_rate: Some(media.output_sample_rate),
        channels: Some(media.channels),
        state,
        position_frames,
        callbacks,
        rendered_frames,
        underflow_callbacks,
        stale_blocks,
        recycle_failures,
        stream_errors,
        worker_error,
        tempo_percent,
        key_lock,
        pitch_semitones,
        tempo_ratio,
        processor_latency_frames,
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot(
    engine: Option<&MixerEngine>,
    loaded_a: Option<&LoadedTrack>,
    loaded_b: Option<&LoadedTrack>,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    cue: CuePreferences,
    routing: &RoutingPreferences,
    output_device_id: Option<&str>,
    output_device_name: Option<&str>,
    device_recoveries: u64,
    device_message: Option<&str>,
) -> MixerServiceSnapshot {
    let Some(engine) = engine else {
        return MixerServiceSnapshot {
            deck_a: unloaded(),
            deck_b: unloaded(),
            crossfader,
            channel_gain_a,
            channel_gain_b,
            master_gain,
            callbacks: 0,
            rendered_frames: 0,
            clipped_samples: 0,
            stream_errors: 0,
            output_device_id: output_device_id.map(str::to_string),
            output_device_name: output_device_name.map(str::to_string),
            device_recoveries,
            device_message: device_message.map(str::to_string),
            cue_a: cue.cue_a,
            cue_b: cue.cue_b,
            cue_blend: cue.blend,
            cue_gain: cue.gain,
            cue_supported: false,
            routing_mode: routing.mode.as_str().to_string(),
            routing_preference: routing.mode.as_str().to_string(),
            routing_limitation: Some(
                "Load a track to open the selected output and detect cue capability.".to_string(),
            ),
            cue_output_device_id: routing.cue_output_device_id.clone(),
            cue_output_device_name: device_name(routing.cue_output_device_id.as_deref()),
            cue_delay_ms: routing.cue_delay_ms,
            cue_callbacks: 0,
            cue_rendered_frames: 0,
            cue_queue_depth_frames: 0,
            cue_min_queue_depth_frames: 0,
            cue_max_queue_depth_frames: 0,
            cue_underflow_callbacks: 0,
            cue_overflow_callbacks: 0,
            cue_stream_errors: 0,
            cue_signal_peak: 0.0,
        };
    };
    let mixer = engine.snapshot();
    let dual = mixer.dual_cue.as_ref();
    MixerServiceSnapshot {
        deck_a: deck_snapshot(engine, DeckId::A, loaded_a),
        deck_b: deck_snapshot(engine, DeckId::B, loaded_b),
        crossfader,
        channel_gain_a,
        channel_gain_b,
        master_gain,
        callbacks: mixer.callbacks,
        rendered_frames: mixer.rendered_frames,
        clipped_samples: mixer.clipped_samples,
        stream_errors: mixer.stream_errors,
        output_device_id: output_device_id.map(str::to_string),
        output_device_name: output_device_name.map(str::to_string),
        device_recoveries,
        device_message: device_message.map(str::to_string),
        cue_a: cue.cue_a && engine.cue_supported(),
        cue_b: cue.cue_b && engine.cue_supported(),
        cue_blend: cue.blend,
        cue_gain: cue.gain,
        cue_supported: engine.cue_supported(),
        routing_mode: if dual.is_some() {
            "dual-device-cue"
        } else if engine.cue_supported() {
            "single-device-cue"
        } else {
            "master-only"
        }
        .to_string(),
        routing_preference: routing.mode.as_str().to_string(),
        routing_limitation: (!engine.cue_supported()).then(|| {
            if dual.is_some() {
                "Dual-device cue stopped; master playback remains active.".to_string()
            } else {
                "Stereo headphone cue requires a four-channel output or an approved dual-device pair."
                    .to_string()
            }
        }),
        cue_output_device_id: routing.cue_output_device_id.clone(),
        cue_output_device_name: device_name(routing.cue_output_device_id.as_deref()),
        cue_delay_ms: routing.cue_delay_ms,
        cue_callbacks: dual.map(|value| value.callbacks).unwrap_or(0),
        cue_rendered_frames: dual.map(|value| value.rendered_frames).unwrap_or(0),
        cue_queue_depth_frames: dual.map(|value| value.queue_depth_frames).unwrap_or(0),
        cue_min_queue_depth_frames: dual
            .map(|value| value.min_queue_depth_frames)
            .unwrap_or(0),
        cue_max_queue_depth_frames: dual
            .map(|value| value.max_queue_depth_frames)
            .unwrap_or(0),
        cue_underflow_callbacks: dual
            .map(|value| value.underflow_callbacks)
            .unwrap_or(0),
        cue_overflow_callbacks: dual
            .map(|value| value.overflow_callbacks)
            .unwrap_or(0),
        cue_stream_errors: dual.map(|value| value.stream_errors).unwrap_or(0),
        cue_signal_peak: dual.map(|value| value.signal_peak).unwrap_or(0.0),
    }
}

fn open_engine(
    device_id: Option<&str>,
    routing: &RoutingPreferences,
) -> Result<MixerEngine, String> {
    if routing.mode == RoutingMode::DualDeviceCue {
        let master = device_id.ok_or_else(|| {
            "select a master output before enabling dual-device cue".to_string()
        })?;
        let cue = routing
            .cue_output_device_id
            .as_deref()
            .ok_or_else(|| "select a cue output before enabling dual-device cue".to_string())?;
        MixerEngine::open_dual_output_devices_unloaded(master, cue, routing.cue_delay_ms)
            .map_err(|error| error.to_string())
    } else {
        MixerEngine::open_output_device_unloaded(device_id).map_err(|error| error.to_string())
    }
}

fn device_name(device_id: Option<&str>) -> Option<String> {
    let id = device_id?;
    djapp_audio_spike::device::output_devices()
        .ok()?
        .into_iter()
        .find(|device| device.id == id)
        .map(|device| device.name)
}

fn default_device() -> Option<(String, String)> {
    djapp_audio_spike::device::output_devices()
        .ok()?
        .into_iter()
        .find(|device| device.is_default)
        .map(|device| (device.id, device.name))
}

#[allow(clippy::too_many_arguments)]
fn restart_engine(
    engine: &mut Option<MixerEngine>,
    device_id: Option<&str>,
    loaded_a: &Option<LoadedTrack>,
    loaded_b: &Option<LoadedTrack>,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    cue: CuePreferences,
    routing: &RoutingPreferences,
) -> Result<(), String> {
    let previous = engine.take();
    let previous_snapshot = previous.as_ref().map(MixerEngine::snapshot);
    let positions = previous.as_ref().map(|engine| {
        [DeckId::A, DeckId::B].map(|deck| {
            let snapshot = match deck {
                DeckId::A => previous_snapshot.as_ref().unwrap().deck_a.clone(),
                DeckId::B => previous_snapshot.as_ref().unwrap().deck_b.clone(),
            };
            let seconds = engine
                .media(deck)
                .map(|media| snapshot.position_frames as f64 / f64::from(media.output_sample_rate))
                .unwrap_or(0.0);
            (
                seconds,
                snapshot.state == DeckState::Playing,
                TempoSettings {
                    tempo_percent: snapshot.tempo_percent,
                    key_lock: snapshot.key_lock,
                    pitch_semitones: snapshot.pitch_semitones,
                },
            )
        })
    });
    drop(previous);

    let primary = build_restored_engine(
        device_id,
        loaded_a,
        loaded_b,
        positions,
        crossfader,
        channel_gain_a,
        channel_gain_b,
        master_gain,
        cue,
        routing,
    );
    match primary {
        Ok(next) => {
            *engine = Some(next);
            Ok(())
        }
        Err(primary_error) if device_id.is_some() => {
            match build_restored_engine(
                None,
                loaded_a,
                loaded_b,
                positions,
                crossfader,
                channel_gain_a,
                channel_gain_b,
                master_gain,
                cue,
                &RoutingPreferences::default(),
            ) {
                Ok(fallback) => {
                    *engine = Some(fallback);
                    Err(format!(
                        "selected output failed: {primary_error}; playback was restored on the macOS default output"
                    ))
                }
                Err(default_error) => Err(format!(
                    "selected output failed: {primary_error}; default output recovery failed: {default_error}"
                )),
            }
        }
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_restored_engine(
    device_id: Option<&str>,
    loaded_a: &Option<LoadedTrack>,
    loaded_b: &Option<LoadedTrack>,
    positions: Option<[(f64, bool, TempoSettings); 2]>,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    cue: CuePreferences,
    routing: &RoutingPreferences,
) -> Result<MixerEngine, String> {
    let mut next = open_engine(device_id, routing)?;
    restore_deck(
        &mut next,
        DeckId::A,
        loaded_a.as_ref(),
        positions.map(|value| value[0]),
    )?;
    restore_deck(
        &mut next,
        DeckId::B,
        loaded_b.as_ref(),
        positions.map(|value| value[1]),
    )?;
    if loaded_a.is_some() {
        next.set_channel_gain(DeckId::A, channel_gain_a)
            .map_err(|error| error.to_string())?;
    }
    if loaded_b.is_some() {
        next.set_channel_gain(DeckId::B, channel_gain_b)
            .map_err(|error| error.to_string())?;
    }
    next.set_crossfader(crossfader)
        .map_err(|error| error.to_string())?;
    next.set_master_gain(master_gain)
        .map_err(|error| error.to_string())?;
    let mut restored_cue = cue;
    apply_automatic_cue(&mut next, crossfader, &mut restored_cue)?;
    Ok(next)
}

fn automatic_cue_selection(crossfader: f32) -> (bool, bool) {
    if crossfader < -0.05 {
        (false, true)
    } else if crossfader > 0.05 {
        (true, false)
    } else {
        (false, false)
    }
}

fn apply_automatic_cue(
    engine: &mut MixerEngine,
    crossfader: f32,
    cue: &mut CuePreferences,
) -> Result<(), String> {
    let (cue_a, cue_b) = automatic_cue_selection(crossfader);
    cue.cue_a = cue_a;
    cue.cue_b = cue_b;
    apply_cue_preferences(engine, *cue)
}

fn apply_cue_preferences(engine: &mut MixerEngine, cue: CuePreferences) -> Result<(), String> {
    if !engine.cue_supported() {
        return Ok(());
    }
    engine
        .set_cue(DeckId::A, cue.cue_a)
        .map_err(|e| e.to_string())?;
    engine
        .set_cue(DeckId::B, cue.cue_b)
        .map_err(|e| e.to_string())?;
    engine.set_cue_blend(cue.blend).map_err(|e| e.to_string())?;
    engine.set_cue_gain(cue.gain).map_err(|e| e.to_string())?;
    Ok(())
}

fn restore_deck(
    engine: &mut MixerEngine,
    deck: DeckId,
    loaded: Option<&LoadedTrack>,
    position: Option<(f64, bool, TempoSettings)>,
) -> Result<(), String> {
    let Some(loaded) = loaded else {
        return Ok(());
    };
    engine
        .load_track(deck, &loaded.path, false)
        .map_err(|error| error.to_string())?;
    if let Some((seconds, playing, settings)) = position {
        engine
            .set_tempo(deck, settings.tempo_percent)
            .map_err(|error| error.to_string())?;
        engine
            .set_key_lock(deck, false)
            .map_err(|error| error.to_string())?;
        engine
            .set_pitch(deck, 0.0)
            .map_err(|error| error.to_string())?;
        engine
            .seek(deck, seconds, playing)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use djapp_audio_spike::{
        analysis::{
            pipeline::WaveformLoudnessProcessor,
            service::AnalysisService,
            types::{AnalysisStage, TrackIdentity},
        },
        persistence::{NewTrack, PersistenceWorker},
    };
    use std::{fs, sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

    #[test]
    fn unloaded_service_reports_both_decks_paused() {
        let service = MixerService::start(
            None,
            CuePreferences::default(),
            RoutingPreferences::default(),
        )
        .unwrap();
        let snapshot = service.snapshot().unwrap();
        assert_eq!(snapshot.deck_a.loaded_track_id, None);
        assert_eq!(snapshot.deck_b.loaded_track_id, None);
        assert_eq!(
            service.play(DeckId::A).unwrap_err(),
            "no track is loaded in either deck"
        );
        service
            .select_cue_output_device("cue-device".to_string())
            .unwrap();
        let routed = service
            .set_routing_mode(RoutingMode::DualDeviceCue)
            .unwrap();
        assert_eq!(routed.routing_mode, "dual-device-cue");
        assert_eq!(routed.cue_output_device_id.as_deref(), Some("cue-device"));
        assert_eq!(service.set_cue_delay(300).unwrap().cue_delay_ms, 250);
    }

    #[test]
    fn crossfader_automatically_cues_the_deck_outside_the_master() {
        assert_eq!(automatic_cue_selection(-1.0), (false, true));
        assert_eq!(automatic_cue_selection(-0.06), (false, true));
        assert_eq!(automatic_cue_selection(0.0), (false, false));
        assert_eq!(automatic_cue_selection(0.05), (false, false));
        assert_eq!(automatic_cue_selection(0.06), (true, false));
        assert_eq!(automatic_cue_selection(1.0), (true, false));
    }

    #[test]
    #[ignore = "requires MacBook Pro Speakers and External Headphones with direct CoreAudio access"]
    fn dual_device_cue_runs_loaded_two_deck_audio() {
        let run_seconds = std::env::var("DJAPP_DUAL_CUE_RUN_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(3);
        let devices = djapp_audio_spike::device::output_devices().unwrap();
        let master = devices
            .iter()
            .find(|device| device.name == "MacBook Pro Speakers")
            .expect("MacBook Pro Speakers must be available");
        let cue_output = devices
            .iter()
            .find(|device| device.name == "External Headphones")
            .expect("External Headphones must be connected");
        let cue_preferences = CuePreferences {
            cue_a: false,
            cue_b: false,
            blend: -1.0,
            gain: 0.1,
        };
        let routing = RoutingPreferences {
            mode: RoutingMode::DualDeviceCue,
            cue_output_device_id: Some(cue_output.id.clone()),
            cue_delay_ms: 0,
        };
        let service = MixerService::start(Some(master.id.clone()), cue_preferences, routing).unwrap();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tests/fixtures/audio");
        service
            .load(DeckId::A, 1, "48 kHz".to_string(), fixtures.join("tone-48k.wav"))
            .unwrap();
        service
            .load(DeckId::B, 2, "96 kHz".to_string(), fixtures.join("tone-96k.wav"))
            .unwrap();
        service.set_channel_gain(DeckId::A, 0.05).unwrap();
        service.set_channel_gain(DeckId::B, 0.05).unwrap();
        service.set_crossfader(-1.0).unwrap();
        service.play(DeckId::A).unwrap();
        service.play(DeckId::B).unwrap();
        thread::sleep(Duration::from_millis(500));
        let before_tempo_change = service.snapshot().unwrap();
        service.set_tempo(DeckId::A, -8.0).unwrap();
        service.set_tempo(DeckId::B, 8.0).unwrap();
        assert!(service.set_key_lock(DeckId::A, true).is_err());
        service.set_key_lock(DeckId::B, false).unwrap();
        assert!(service.set_pitch(DeckId::B, 3.0).is_err());
        thread::sleep(Duration::from_millis(500));
        let after_tempo_change = service.snapshot().unwrap();
        assert!(
            after_tempo_change.deck_a.position_frames >= before_tempo_change.deck_a.position_frames,
            "Deck A moved backward after its live tempo change"
        );
        assert!(
            after_tempo_change.deck_b.position_frames >= before_tempo_change.deck_b.position_frames,
            "Deck B moved backward after its live tempo change"
        );
        let started = Instant::now();
        let mut next_seek = Duration::from_millis(1_500);
        let mut next_report = Duration::from_secs(60);
        let mut baseline_relative_frames = None;
        let mut max_relative_deviation = 0_u64;
        let mut max_signal_peak = 0.0_f32;
        while started.elapsed() < Duration::from_secs(run_seconds) {
            thread::sleep(Duration::from_millis(500));
            let elapsed = started.elapsed();
            if elapsed >= next_seek {
                service.seek(DeckId::A, 0.0, true).unwrap();
                service.seek(DeckId::B, 0.0, true).unwrap();
                next_seek += Duration::from_millis(1_500);
            }
            let current = service.snapshot().unwrap();
            max_signal_peak = max_signal_peak.max(current.cue_signal_peak);
            let relative = current
                .rendered_frames
                .abs_diff(current.cue_rendered_frames);
            let baseline = *baseline_relative_frames.get_or_insert(relative);
            max_relative_deviation = max_relative_deviation.max(relative.abs_diff(baseline));
            assert_eq!(current.routing_mode, "dual-device-cue");
            assert_eq!(current.stream_errors, 0);
            assert_eq!(current.cue_stream_errors, 0);
            assert_eq!(current.cue_underflow_callbacks, 0);
            assert_eq!(current.cue_overflow_callbacks, 0);
            assert_eq!(current.deck_a.tempo_percent, -8.0);
            assert_eq!(current.deck_b.tempo_percent, 8.0);
            assert!(!current.deck_b.key_lock);
            assert_eq!(current.deck_b.pitch_semitones, 0.0);
            if elapsed >= next_report {
                println!(
                    "elapsed_seconds={} master_frames={} cue_frames={} relative_frames={} cue_depth={} cue_min={} cue_max={}",
                    elapsed.as_secs(),
                    current.rendered_frames,
                    current.cue_rendered_frames,
                    relative,
                    current.cue_queue_depth_frames,
                    current.cue_min_queue_depth_frames,
                    current.cue_max_queue_depth_frames,
                );
                next_report += Duration::from_secs(60);
            }
        }
        let snapshot = service.snapshot().unwrap();
        println!(
            "run_seconds={} master_callbacks={} master_frames={} master_errors={} cue_callbacks={} cue_frames={} relative_frames={} max_relative_deviation={} cue_depth={} cue_min={} cue_max={} cue_underflows={} cue_overflows={} cue_errors={} cue_signal_peak={}",
            run_seconds,
            snapshot.callbacks,
            snapshot.rendered_frames,
            snapshot.stream_errors,
            snapshot.cue_callbacks,
            snapshot.cue_rendered_frames,
            snapshot.rendered_frames.abs_diff(snapshot.cue_rendered_frames),
            max_relative_deviation,
            snapshot.cue_queue_depth_frames,
            snapshot.cue_min_queue_depth_frames,
            snapshot.cue_max_queue_depth_frames,
            snapshot.cue_underflow_callbacks,
            snapshot.cue_overflow_callbacks,
            snapshot.cue_stream_errors,
            max_signal_peak,
        );
        assert_eq!(snapshot.routing_mode, "dual-device-cue");
        assert!(snapshot.callbacks > 0);
        assert!(snapshot.cue_callbacks > 0);
        assert_eq!(snapshot.stream_errors, 0);
        assert_eq!(snapshot.cue_stream_errors, 0);
        assert_eq!(snapshot.cue_underflow_callbacks, 0);
        assert_eq!(snapshot.cue_overflow_callbacks, 0);
        assert!(snapshot.cue_queue_depth_frames < 4_096);
        assert!(max_signal_peak > 0.0001, "cue callback consumed only silence");
    }

    #[test]
    #[ignore = "requires direct CoreAudio device access"]
    fn mixed_rate_two_deck_commands_share_one_healthy_stream() {
        let service = MixerService::start(
            None,
            CuePreferences::default(),
            RoutingPreferences::default(),
        )
        .unwrap();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tests/fixtures/audio");
        service
            .load(
                DeckId::A,
                1,
                "48 kHz".to_string(),
                fixtures.join("tone-48k.wav"),
            )
            .unwrap();
        let routing = service.snapshot().unwrap();
        assert!(!routing.cue_supported);
        assert_eq!(routing.routing_mode, "master-only");
        assert!(service.set_cue(DeckId::A, true).is_err());
        service
            .load(
                DeckId::B,
                2,
                "96 kHz".to_string(),
                fixtures.join("tone-96k.wav"),
            )
            .unwrap();
        service.set_channel_gain(DeckId::A, 0.0).unwrap();
        service.set_channel_gain(DeckId::B, 0.0).unwrap();
        service.play(DeckId::A).unwrap();
        service.play(DeckId::B).unwrap();
        thread::sleep(Duration::from_millis(700));
        let before_tempo_change = service.snapshot().unwrap();
        service.set_tempo(DeckId::A, -8.0).unwrap();
        service.set_tempo(DeckId::B, 8.0).unwrap();
        assert!(service.set_key_lock(DeckId::A, true).is_err());
        service.set_key_lock(DeckId::B, false).unwrap();
        assert!(service.set_pitch(DeckId::B, 3.0).is_err());
        thread::sleep(Duration::from_millis(500));
        let after_tempo_change = service.snapshot().unwrap();
        assert!(
            after_tempo_change.deck_a.position_frames > before_tempo_change.deck_a.position_frames,
            "Deck A did not progress through its live tempo change"
        );
        assert!(
            after_tempo_change.deck_b.position_frames > before_tempo_change.deck_b.position_frames,
            "Deck B did not progress through its live tempo change"
        );
        assert_eq!(after_tempo_change.deck_a.tempo_percent, -8.0);
        assert_eq!(after_tempo_change.deck_b.tempo_percent, 8.0);
        assert!(!after_tempo_change.deck_b.key_lock);
        assert_eq!(after_tempo_change.deck_b.pitch_semitones, 0.0);
        let before_switch = service.snapshot().unwrap();
        let default_device = djapp_audio_spike::device::output_devices()
            .unwrap()
            .into_iter()
            .find(|device| device.is_default)
            .unwrap();
        service.select_output_device(default_device.id).unwrap();
        thread::sleep(Duration::from_millis(350));
        let after_switch = service.snapshot().unwrap();
        assert_eq!(after_switch.deck_a.state, DeckState::Playing);
        assert_eq!(after_switch.deck_b.state, DeckState::Playing);
        assert!(after_switch.deck_a.position_frames > before_switch.deck_a.position_frames);
        assert!(after_switch.deck_b.position_frames > before_switch.deck_b.position_frames);
        service.set_crossfader(0.5).unwrap();
        service.seek(DeckId::B, 0.5, true).unwrap();
        thread::sleep(Duration::from_millis(350));
        let stopped = service.stop(DeckId::A).unwrap();
        println!(
            "callbacks={} clipped={} stream_errors={} deck_a_underflows={} deck_b_underflows={} deck_a_stale={} deck_b_stale={} deck_a_recycle={} deck_b_recycle={}",
            stopped.callbacks,
            stopped.clipped_samples,
            stopped.stream_errors,
            stopped.deck_a.underflow_callbacks,
            stopped.deck_b.underflow_callbacks,
            stopped.deck_a.stale_blocks,
            stopped.deck_b.stale_blocks,
            stopped.deck_a.recycle_failures,
            stopped.deck_b.recycle_failures,
        );
        assert!(stopped.callbacks > 0);
        assert_eq!(stopped.stream_errors, 0);
        assert_eq!(stopped.deck_a.underflow_callbacks, 0);
        assert_eq!(stopped.deck_b.underflow_callbacks, 0);
        assert_eq!(stopped.deck_a.recycle_failures, 0);
        assert_eq!(stopped.deck_b.recycle_failures, 0);
        assert_eq!(stopped.deck_a.worker_error, None);
        assert_eq!(stopped.deck_b.worker_error, None);
    }

    #[test]
    #[ignore = "requires direct CoreAudio device access on the Apple M3 target"]
    fn two_deck_playback_remains_healthy_during_full_track_analysis() {
        let service = MixerService::start(
            None,
            CuePreferences::default(),
            RoutingPreferences::default(),
        )
        .unwrap();
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tests/fixtures/audio");
        service
            .load(DeckId::A, 1, "48 kHz".to_string(), fixtures.join("tone-48k.wav"))
            .unwrap();
        service
            .load(DeckId::B, 2, "96 kHz".to_string(), fixtures.join("tone-96k.wav"))
            .unwrap();
        service.set_channel_gain(DeckId::A, 0.0).unwrap();
        service.set_channel_gain(DeckId::B, 0.0).unwrap();
        service.play(DeckId::A).unwrap();
        service.play(DeckId::B).unwrap();
        thread::sleep(Duration::from_millis(500));
        let before = service.snapshot().unwrap();

        let root = std::env::temp_dir().join(format!(
            "djapp-analysis-playback-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let source = root.join("music.wav");
        fs::copy(fixtures.join("music-like-48k.wav"), &source).unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let modified_at_ms = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let persistence = Arc::new(PersistenceWorker::start(root.join("library.sqlite")).unwrap());
        let track_id = persistence
            .upsert_track(NewTrack {
                library_root_id: None,
                path: source.to_string_lossy().into_owned(),
                file_size: metadata.len() as i64,
                modified_at_ms,
                content_fingerprint: None,
                title: Some("Analysis load".to_string()),
                artist: None,
                album: None,
                genre: None,
                duration_frames: None,
                sample_rate: Some(48_000),
                channels: Some(2),
                codec: Some("pcm_s16le".to_string()),
                missing: false,
                updated_at_ms: modified_at_ms,
            })
            .unwrap()
            .id;
        let analysis = AnalysisService::start(
            Arc::clone(&persistence),
            WaveformLoudnessProcessor::new(root.join("cache")),
        )
        .unwrap();
        analysis
            .enqueue(TrackIdentity {
                track_id,
                path: source,
                file_size: metadata.len(),
                modified_at_ms,
                content_fingerprint: None,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if analysis.snapshots().unwrap().iter().any(|snapshot| {
                snapshot.track_id == track_id && snapshot.stage == AnalysisStage::Complete
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "analysis did not finish during playback");
            thread::sleep(Duration::from_millis(50));
        }
        let after = service.snapshot().unwrap();
        println!(
            "analysis_playback callbacks_before={} callbacks_after={} stream_errors={} deck_a_underflows={} deck_b_underflows={} deck_a_recycle={} deck_b_recycle={}",
            before.callbacks,
            after.callbacks,
            after.stream_errors,
            after.deck_a.underflow_callbacks,
            after.deck_b.underflow_callbacks,
            after.deck_a.recycle_failures,
            after.deck_b.recycle_failures,
        );
        assert!(after.callbacks > before.callbacks);
        assert_eq!(after.stream_errors, before.stream_errors);
        assert_eq!(after.deck_a.underflow_callbacks, before.deck_a.underflow_callbacks);
        assert_eq!(after.deck_b.underflow_callbacks, before.deck_b.underflow_callbacks);
        assert_eq!(after.deck_a.recycle_failures, before.deck_a.recycle_failures);
        assert_eq!(after.deck_b.recycle_failures, before.deck_b.recycle_failures);
        assert_eq!(after.deck_a.worker_error, None);
        assert_eq!(after.deck_b.worker_error, None);
        assert_eq!(persistence.analysis(track_id).unwrap().unwrap().status, "complete");
        analysis.shutdown().unwrap();
        drop(persistence);
        fs::remove_dir_all(root).unwrap();
    }
}
