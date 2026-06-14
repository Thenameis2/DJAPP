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
    pub underflow_callbacks: u64,
    pub stale_blocks: u64,
    pub recycle_failures: u64,
    pub stream_errors: u64,
    pub worker_error: Option<String>,
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
    pub routing_limitation: Option<String>,
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
    ) -> Result<Self, String> {
        let (sender, receiver) = mpsc::channel();
        let join = thread::Builder::new()
            .name("djapp-mixer-service".to_string())
            .spawn(move || run(receiver, preferred_output_device, cue_preferences))
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
                        match open_engine(output_device_id.as_deref()) {
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
                        apply_cue_preferences(engine.as_mut().unwrap(), cue)?;
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
                output_device_id.as_deref(),
                output_device_name.as_deref(),
                device_recoveries,
                device_message.as_deref(),
                |engine| engine.stop(deck),
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
                            .map_err(|error| error.to_string())
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
                    )
                } else {
                    open_engine(Some(&device_id)).map(drop)
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
                        output_device_id.as_deref(),
                        output_device_name.as_deref(),
                        device_recoveries,
                        device_message.as_deref(),
                    )
                }));
                continue;
            }
            Command::Snapshot { response } => {
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
        underflow_callbacks: 0,
        stale_blocks: 0,
        recycle_failures: 0,
        stream_errors: 0,
        worker_error: None,
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
        underflow_callbacks,
        stale_blocks,
        recycle_failures,
        stream_errors,
        worker_error,
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
        underflow_callbacks,
        stale_blocks,
        recycle_failures,
        stream_errors,
        worker_error,
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
            routing_mode: "master-only".to_string(),
            routing_limitation: Some(
                "Load a track to open the selected output and detect cue capability.".to_string(),
            ),
        };
    };
    let mixer = engine.snapshot();
    MixerServiceSnapshot {
        deck_a: deck_snapshot(engine, DeckId::A, loaded_a),
        deck_b: deck_snapshot(engine, DeckId::B, loaded_b),
        crossfader,
        channel_gain_a,
        channel_gain_b,
        master_gain,
        callbacks: mixer.callbacks,
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
        routing_mode: if engine.cue_supported() {
            "master-and-cue"
        } else {
            "master-only"
        }
        .to_string(),
        routing_limitation: (!engine.cue_supported()).then(|| {
            "Stereo headphone cue requires one output device with at least four channels."
                .to_string()
        }),
    }
}

fn open_engine(device_id: Option<&str>) -> Result<MixerEngine, String> {
    MixerEngine::open_output_device_unloaded(device_id).map_err(|error| error.to_string())
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
            (seconds, snapshot.state == DeckState::Playing)
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
    positions: Option<[(f64, bool); 2]>,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    cue: CuePreferences,
) -> Result<MixerEngine, String> {
    let mut next = open_engine(device_id)?;
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
    apply_cue_preferences(&mut next, cue)?;
    Ok(next)
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
    position: Option<(f64, bool)>,
) -> Result<(), String> {
    let Some(loaded) = loaded else {
        return Ok(());
    };
    engine
        .load_track(deck, &loaded.path, false)
        .map_err(|error| error.to_string())?;
    if let Some((seconds, playing)) = position {
        engine
            .seek(deck, seconds, playing)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn unloaded_service_reports_both_decks_paused() {
        let service = MixerService::start(None, CuePreferences::default()).unwrap();
        let snapshot = service.snapshot().unwrap();
        assert_eq!(snapshot.deck_a.loaded_track_id, None);
        assert_eq!(snapshot.deck_b.loaded_track_id, None);
        assert_eq!(
            service.play(DeckId::A).unwrap_err(),
            "no track is loaded in either deck"
        );
    }

    #[test]
    #[ignore = "requires direct CoreAudio device access"]
    fn mixed_rate_two_deck_commands_share_one_healthy_stream() {
        let service = MixerService::start(None, CuePreferences::default()).unwrap();
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
}
