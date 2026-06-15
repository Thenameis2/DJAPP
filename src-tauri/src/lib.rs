use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use djapp_audio_spike::{
    library::{scan_library_root, ScanSummary},
    mixer::DeckId,
    persistence::{PersistenceWorker, TrackRecord, SCHEMA_VERSION},
};
use serde::Serialize;
use tauri::Manager;

mod mixer_service;

use mixer_service::{
    CuePreferences, DeckServiceSnapshot, MixerService, MixerServiceSnapshot, RoutingMode,
    RoutingPreferences,
};

const OUTPUT_DEVICE_SETTING: &str = "audio.output_device_id";
const CUE_A_SETTING: &str = "audio.cue_a";
const CUE_B_SETTING: &str = "audio.cue_b";
const CUE_BLEND_SETTING: &str = "audio.cue_blend";
const CUE_GAIN_SETTING: &str = "audio.cue_gain";
const ROUTING_MODE_SETTING: &str = "audio.routing_mode";
const CUE_OUTPUT_DEVICE_SETTING: &str = "audio.cue_output_device_id";
const CUE_DELAY_SETTING: &str = "audio.cue_delay_ms";

struct AppState {
    persistence: Arc<PersistenceWorker>,
    mixer: Arc<MixerService>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanResult {
    library_root_id: i64,
    root_path: String,
    discovered: usize,
    inserted: usize,
    modified: usize,
    restored: usize,
    unchanged: usize,
    missing: usize,
    metadata_failures: usize,
    traversal_errors: Vec<String>,
}

impl From<ScanSummary> for ScanResult {
    fn from(summary: ScanSummary) -> Self {
        Self {
            library_root_id: summary.library_root_id,
            root_path: summary.root_path,
            discovered: summary.discovered,
            inserted: summary.inserted,
            modified: summary.modified,
            restored: summary.restored,
            unchanged: summary.unchanged,
            missing: summary.missing,
            metadata_failures: summary.metadata_failures,
            traversal_errors: summary.traversal_errors,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackView {
    id: i64,
    path: String,
    title: String,
    artist: Option<String>,
    duration_seconds: Option<f64>,
    codec: Option<String>,
    missing: bool,
}

impl From<TrackRecord> for TrackView {
    fn from(track: TrackRecord) -> Self {
        let title = track.title.unwrap_or_else(|| title_from_path(&track.path));
        let duration_seconds = track
            .duration_frames
            .zip(track.sample_rate)
            .filter(|(_, rate)| *rate > 0)
            .map(|(frames, rate)| frames as f64 / rate as f64);
        Self {
            id: track.id,
            path: track.path,
            title,
            artist: track.artist,
            duration_seconds,
            codec: track.codec,
            missing: track.missing,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeckView {
    loaded_track_id: Option<i64>,
    title: Option<String>,
    path: Option<String>,
    duration_seconds: Option<f64>,
    position_seconds: f64,
    sample_rate: Option<u32>,
    channels: Option<usize>,
    state: &'static str,
    callbacks: u64,
    rendered_frames: u64,
    underflow_callbacks: u64,
    stale_blocks: u64,
    recycle_failures: u64,
    stream_errors: u64,
    worker_error: Option<String>,
    tempo_percent: f32,
    key_lock: bool,
    pitch_semitones: f32,
    tempo_ratio: f64,
    processor_latency_ms: f64,
}

impl From<DeckServiceSnapshot> for DeckView {
    fn from(snapshot: DeckServiceSnapshot) -> Self {
        let position_seconds = snapshot
            .sample_rate
            .filter(|rate| *rate > 0)
            .map(|rate| snapshot.position_frames as f64 / f64::from(rate))
            .unwrap_or(0.0);
        Self {
            loaded_track_id: snapshot.loaded_track_id,
            title: snapshot.title,
            path: snapshot.path,
            duration_seconds: snapshot.duration_seconds,
            position_seconds,
            sample_rate: snapshot.sample_rate,
            channels: snapshot.channels,
            state: match snapshot.state {
                djapp_audio_spike::deck::DeckState::Paused => "paused",
                djapp_audio_spike::deck::DeckState::Playing => "playing",
                djapp_audio_spike::deck::DeckState::Ended => "ended",
            },
            callbacks: snapshot.callbacks,
            rendered_frames: snapshot.rendered_frames,
            underflow_callbacks: snapshot.underflow_callbacks,
            stale_blocks: snapshot.stale_blocks,
            recycle_failures: snapshot.recycle_failures,
            stream_errors: snapshot.stream_errors,
            worker_error: snapshot.worker_error,
            tempo_percent: snapshot.tempo_percent,
            key_lock: snapshot.key_lock,
            pitch_semitones: snapshot.pitch_semitones,
            tempo_ratio: snapshot.tempo_ratio,
            processor_latency_ms: snapshot
                .sample_rate
                .filter(|rate| *rate > 0)
                .map(|rate| snapshot.processor_latency_frames as f64 * 1_000.0 / f64::from(rate))
                .unwrap_or(0.0),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MixerView {
    deck_a: DeckView,
    deck_b: DeckView,
    crossfader: f32,
    channel_gain_a: f32,
    channel_gain_b: f32,
    master_gain: f32,
    callbacks: u64,
    rendered_frames: u64,
    clipped_samples: u64,
    stream_errors: u64,
    output_device_id: Option<String>,
    output_device_name: Option<String>,
    device_recoveries: u64,
    device_message: Option<String>,
    cue_a: bool,
    cue_b: bool,
    cue_blend: f32,
    cue_gain: f32,
    cue_supported: bool,
    routing_mode: String,
    routing_preference: String,
    routing_limitation: Option<String>,
    cue_output_device_id: Option<String>,
    cue_output_device_name: Option<String>,
    cue_delay_ms: u32,
    cue_callbacks: u64,
    cue_rendered_frames: u64,
    cue_queue_depth_frames: u64,
    cue_min_queue_depth_frames: u64,
    cue_max_queue_depth_frames: u64,
    cue_underflow_callbacks: u64,
    cue_overflow_callbacks: u64,
    cue_stream_errors: u64,
    cue_signal_peak: f32,
}

impl From<MixerServiceSnapshot> for MixerView {
    fn from(snapshot: MixerServiceSnapshot) -> Self {
        Self {
            deck_a: snapshot.deck_a.into(),
            deck_b: snapshot.deck_b.into(),
            crossfader: snapshot.crossfader,
            channel_gain_a: snapshot.channel_gain_a,
            channel_gain_b: snapshot.channel_gain_b,
            master_gain: snapshot.master_gain,
            callbacks: snapshot.callbacks,
            rendered_frames: snapshot.rendered_frames,
            clipped_samples: snapshot.clipped_samples,
            stream_errors: snapshot.stream_errors,
            output_device_id: snapshot.output_device_id,
            output_device_name: snapshot.output_device_name,
            device_recoveries: snapshot.device_recoveries,
            device_message: snapshot.device_message,
            cue_a: snapshot.cue_a,
            cue_b: snapshot.cue_b,
            cue_blend: snapshot.cue_blend,
            cue_gain: snapshot.cue_gain,
            cue_supported: snapshot.cue_supported,
            routing_mode: snapshot.routing_mode,
            routing_preference: snapshot.routing_preference,
            routing_limitation: snapshot.routing_limitation,
            cue_output_device_id: snapshot.cue_output_device_id,
            cue_output_device_name: snapshot.cue_output_device_name,
            cue_delay_ms: snapshot.cue_delay_ms,
            cue_callbacks: snapshot.cue_callbacks,
            cue_rendered_frames: snapshot.cue_rendered_frames,
            cue_queue_depth_frames: snapshot.cue_queue_depth_frames,
            cue_min_queue_depth_frames: snapshot.cue_min_queue_depth_frames,
            cue_max_queue_depth_frames: snapshot.cue_max_queue_depth_frames,
            cue_underflow_callbacks: snapshot.cue_underflow_callbacks,
            cue_overflow_callbacks: snapshot.cue_overflow_callbacks,
            cue_stream_errors: snapshot.cue_stream_errors,
            cue_signal_peak: snapshot.cue_signal_peak,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputDeviceView {
    id: String,
    name: String,
    is_default: bool,
    interface: String,
    channels: u16,
    max_channels: u16,
    sample_rate: u32,
    stereo_master_supported: bool,
    stereo_cue_supported: bool,
    routing_mode: String,
    limitation: Option<String>,
}

impl From<djapp_audio_spike::device::OutputDeviceInfo> for OutputDeviceView {
    fn from(device: djapp_audio_spike::device::OutputDeviceInfo) -> Self {
        Self {
            id: device.id,
            name: device.name,
            is_default: device.is_default,
            interface: device.interface,
            channels: device.channels,
            max_channels: device.max_channels,
            sample_rate: device.sample_rate,
            stereo_master_supported: device.stereo_master_supported,
            stereo_cue_supported: device.stereo_cue_supported,
            routing_mode: device.routing_mode,
            limitation: device.limitation,
        }
    }
}

#[tauri::command]
fn engine_status() -> String {
    format!("Rust engine online. SQLite schema version {SCHEMA_VERSION} is ready.")
}

#[tauri::command]
async fn scan_music_folder(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<ScanResult, String> {
    let persistence = Arc::clone(&state.persistence);
    tauri::async_runtime::spawn_blocking(move || scan_library_root(path, &persistence))
        .await
        .map_err(|error| format!("library scan worker failed: {error}"))?
        .map(ScanResult::from)
}

#[tauri::command]
fn library_tracks(state: tauri::State<'_, AppState>) -> Result<Vec<TrackView>, String> {
    state
        .persistence
        .tracks()
        .map(|tracks| tracks.into_iter().map(TrackView::from).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn audio_output_devices() -> Result<Vec<OutputDeviceView>, String> {
    djapp_audio_spike::device::output_devices()
        .map(|devices| devices.into_iter().map(OutputDeviceView::from).collect())
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn audio_select_output_device(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let mixer = Arc::clone(&state.mixer);
    let selected = device_id.clone();
    let snapshot =
        tauri::async_runtime::spawn_blocking(move || mixer.select_output_device(selected))
            .await
            .map_err(|error| format!("mixer worker failed: {error}"))??;
    let updated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as i64;
    state
        .persistence
        .set_setting(OUTPUT_DEVICE_SETTING.to_string(), device_id, updated_at_ms)
        .map_err(|error| error.to_string())?;
    Ok(snapshot.into())
}

#[tauri::command]
async fn audio_select_cue_output_device(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let mixer = Arc::clone(&state.mixer);
    let selected = device_id.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || {
        mixer.select_cue_output_device(selected)
    })
    .await
    .map_err(|error| format!("mixer worker failed: {error}"))??;
    persist_mixer_setting(&state, CUE_OUTPUT_DEVICE_SETTING, device_id).await?;
    Ok(snapshot.into())
}

#[tauri::command]
async fn audio_set_routing_mode(
    mode: String,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let parsed = RoutingMode::parse(&mode)?;
    let view = mixer_command(
        move |mixer| mixer.set_routing_mode(parsed),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, ROUTING_MODE_SETTING, mode).await?;
    Ok(view)
}

#[tauri::command]
async fn audio_set_cue_delay_ms(
    delay_ms: u32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let delay_ms = delay_ms.min(250);
    let view = mixer_command(
        move |mixer| mixer.set_cue_delay(delay_ms),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, CUE_DELAY_SETTING, delay_ms.to_string()).await?;
    Ok(view)
}

fn indexed_track(
    track_id: i64,
    persistence: &PersistenceWorker,
) -> Result<(String, PathBuf), String> {
    let track = persistence
        .track(track_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "track is not indexed in the local library".to_string())?;
    if track.missing {
        return Err("track is marked missing; rescan its folder before loading".to_string());
    }
    let path = PathBuf::from(&track.path);
    if !path.is_file() {
        return Err("track file no longer exists; rescan its folder".to_string());
    }
    Ok((
        track.title.unwrap_or_else(|| title_from_path(&track.path)),
        path,
    ))
}

fn title_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Unknown track".to_string())
}

async fn mixer_command(
    operation: impl FnOnce(&MixerService) -> Result<MixerServiceSnapshot, String> + Send + 'static,
    mixer: Arc<MixerService>,
) -> Result<MixerView, String> {
    tauri::async_runtime::spawn_blocking(move || operation(&mixer))
        .await
        .map_err(|error| format!("mixer worker failed: {error}"))?
        .map(MixerView::from)
}

async fn load_deck(
    track_id: i64,
    deck: DeckId,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let (title, path) = indexed_track(track_id, &state.persistence)?;
    mixer_command(
        move |mixer| mixer.load(deck, track_id, title, path),
        Arc::clone(&state.mixer),
    )
    .await
}

#[tauri::command]
async fn deck_a_load(
    track_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    load_deck(track_id, DeckId::A, state).await
}
#[tauri::command]
async fn deck_b_load(
    track_id: i64,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    load_deck(track_id, DeckId::B, state).await
}

macro_rules! deck_command {
    ($name:ident, $method:ident, $deck:expr) => {
        #[tauri::command]
        async fn $name(state: tauri::State<'_, AppState>) -> Result<MixerView, String> {
            mixer_command(|mixer| mixer.$method($deck), Arc::clone(&state.mixer)).await
        }
    };
}
deck_command!(deck_a_play, play, DeckId::A);
deck_command!(deck_a_pause, pause, DeckId::A);
deck_command!(deck_a_stop, stop, DeckId::A);
deck_command!(deck_b_play, play, DeckId::B);
deck_command!(deck_b_pause, pause, DeckId::B);
deck_command!(deck_b_stop, stop, DeckId::B);

async fn seek_deck(
    seconds: f64,
    resume: bool,
    deck: DeckId,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err("seek position must be finite and non-negative".to_string());
    }
    mixer_command(
        move |mixer| mixer.seek(deck, seconds, resume),
        Arc::clone(&state.mixer),
    )
    .await
}
#[tauri::command]
async fn deck_a_seek(
    seconds: f64,
    resume: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    seek_deck(seconds, resume, DeckId::A, state).await
}
#[tauri::command]
async fn deck_b_seek(
    seconds: f64,
    resume: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    seek_deck(seconds, resume, DeckId::B, state).await
}

#[tauri::command]
async fn mixer_snapshot(state: tauri::State<'_, AppState>) -> Result<MixerView, String> {
    mixer_command(|mixer| mixer.snapshot(), Arc::clone(&state.mixer)).await
}
#[tauri::command]
async fn mixer_set_crossfader(
    value: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_crossfader(value),
        Arc::clone(&state.mixer),
    )
    .await
}
#[tauri::command]
async fn mixer_set_master_gain(
    gain: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_master_gain(gain),
        Arc::clone(&state.mixer),
    )
    .await
}
#[tauri::command]
async fn deck_a_set_gain(
    gain: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_channel_gain(DeckId::A, gain),
        Arc::clone(&state.mixer),
    )
    .await
}
#[tauri::command]
async fn deck_b_set_gain(
    gain: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_channel_gain(DeckId::B, gain),
        Arc::clone(&state.mixer),
    )
    .await
}

async fn set_deck_tempo(
    percent: f32,
    deck: DeckId,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_tempo(deck, percent),
        Arc::clone(&state.mixer),
    )
    .await
}

#[tauri::command]
async fn deck_a_set_tempo(
    percent: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_tempo(percent, DeckId::A, state).await
}

#[tauri::command]
async fn deck_b_set_tempo(
    percent: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_tempo(percent, DeckId::B, state).await
}

async fn set_deck_key_lock(
    enabled: bool,
    deck: DeckId,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_key_lock(deck, enabled),
        Arc::clone(&state.mixer),
    )
    .await
}

#[tauri::command]
async fn deck_a_set_key_lock(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_key_lock(enabled, DeckId::A, state).await
}

#[tauri::command]
async fn deck_b_set_key_lock(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_key_lock(enabled, DeckId::B, state).await
}

async fn set_deck_pitch(
    semitones: f32,
    deck: DeckId,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    mixer_command(
        move |mixer| mixer.set_pitch(deck, semitones),
        Arc::clone(&state.mixer),
    )
    .await
}

#[tauri::command]
async fn deck_a_set_pitch(
    semitones: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_pitch(semitones, DeckId::A, state).await
}

#[tauri::command]
async fn deck_b_set_pitch(
    semitones: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    set_deck_pitch(semitones, DeckId::B, state).await
}

async fn persist_mixer_setting(
    state: &tauri::State<'_, AppState>,
    key: &str,
    value: String,
) -> Result<(), String> {
    let updated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis() as i64;
    state
        .persistence
        .set_setting(key.to_string(), value, updated_at_ms)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn deck_a_set_cue(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let view = mixer_command(
        move |mixer| mixer.set_cue(DeckId::A, enabled),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, CUE_A_SETTING, enabled.to_string()).await?;
    Ok(view)
}

#[tauri::command]
async fn deck_b_set_cue(
    enabled: bool,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let view = mixer_command(
        move |mixer| mixer.set_cue(DeckId::B, enabled),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, CUE_B_SETTING, enabled.to_string()).await?;
    Ok(view)
}

#[tauri::command]
async fn mixer_set_cue_blend(
    value: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let view = mixer_command(
        move |mixer| mixer.set_cue_blend(value),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, CUE_BLEND_SETTING, value.to_string()).await?;
    Ok(view)
}

#[tauri::command]
async fn mixer_set_cue_gain(
    gain: f32,
    state: tauri::State<'_, AppState>,
) -> Result<MixerView, String> {
    let view = mixer_command(
        move |mixer| mixer.set_cue_gain(gain),
        Arc::clone(&state.mixer),
    )
    .await?;
    persist_mixer_setting(&state, CUE_GAIN_SETTING, gain.to_string()).await?;
    Ok(view)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&app_data_dir)?;
            let persistence = PersistenceWorker::start(app_data_dir.join("djapp.sqlite"))?;
            let preferred_output_device = persistence
                .setting(OUTPUT_DEVICE_SETTING.to_string())
                .map_err(std::io::Error::other)?;
            let cue_preferences = CuePreferences {
                cue_a: setting_bool(&persistence, CUE_A_SETTING, false)?,
                cue_b: setting_bool(&persistence, CUE_B_SETTING, false)?,
                blend: setting_f32(&persistence, CUE_BLEND_SETTING, -1.0)?,
                gain: setting_f32(&persistence, CUE_GAIN_SETTING, 0.5)?,
            };
            let routing_preferences = RoutingPreferences {
                mode: persistence
                    .setting(ROUTING_MODE_SETTING.to_string())
                    .map_err(std::io::Error::other)?
                    .as_deref()
                    .map(RoutingMode::parse)
                    .transpose()
                    .map_err(std::io::Error::other)?
                    .unwrap_or(RoutingMode::Automatic),
                cue_output_device_id: persistence
                    .setting(CUE_OUTPUT_DEVICE_SETTING.to_string())
                    .map_err(std::io::Error::other)?,
                cue_delay_ms: setting_u32(&persistence, CUE_DELAY_SETTING, 0)?.min(250),
            };
            let mixer = MixerService::start(
                preferred_output_device,
                cue_preferences,
                routing_preferences,
            )
            .map_err(std::io::Error::other)?;
            app.manage(AppState {
                persistence: Arc::new(persistence),
                mixer: Arc::new(mixer),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            engine_status,
            scan_music_folder,
            library_tracks,
            audio_output_devices,
            audio_select_output_device,
            audio_select_cue_output_device,
            audio_set_routing_mode,
            audio_set_cue_delay_ms,
            deck_a_load,
            deck_b_load,
            deck_a_play,
            deck_a_pause,
            deck_a_seek,
            deck_a_stop,
            deck_b_play,
            deck_b_pause,
            deck_b_seek,
            deck_b_stop,
            mixer_snapshot,
            mixer_set_crossfader,
            mixer_set_master_gain,
            deck_a_set_gain,
            deck_b_set_gain,
            deck_a_set_tempo,
            deck_b_set_tempo,
            deck_a_set_key_lock,
            deck_b_set_key_lock,
            deck_a_set_pitch,
            deck_b_set_pitch,
            deck_a_set_cue,
            deck_b_set_cue,
            mixer_set_cue_blend,
            mixer_set_cue_gain
        ])
        .run(tauri::generate_context!())
        .expect("error while running DJ App");
}

fn setting_bool(
    persistence: &PersistenceWorker,
    key: &str,
    fallback: bool,
) -> Result<bool, std::io::Error> {
    Ok(persistence
        .setting(key.to_string())
        .map_err(std::io::Error::other)?
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback))
}

fn setting_f32(
    persistence: &PersistenceWorker,
    key: &str,
    fallback: f32,
) -> Result<f32, std::io::Error> {
    Ok(persistence
        .setting(key.to_string())
        .map_err(std::io::Error::other)?
        .and_then(|value| value.parse().ok())
        .filter(|value: &f32| value.is_finite())
        .unwrap_or(fallback))
}

fn setting_u32(
    persistence: &PersistenceWorker,
    key: &str,
    fallback: u32,
) -> Result<u32, std::io::Error> {
    Ok(persistence
        .setting(key.to_string())
        .map_err(std::io::Error::other)?
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback))
}
