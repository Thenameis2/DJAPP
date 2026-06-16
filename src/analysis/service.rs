use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::persistence::{AnalysisRecord, PersistenceWorker};

use super::{
    types::{AnalysisStage, MusicalMode, TrackAnalysisResult, TrackIdentity},
    ANALYSIS_VERSION,
};

pub const DEFAULT_ANALYSIS_QUEUE_CAPACITY: usize = 64;
const CANCELLED_MESSAGE: &str = "analysis cancelled";

#[derive(Clone, Debug)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub trait AnalysisProcessor: Send + Sync + 'static {
    fn analyze(
        &self,
        track: &TrackIdentity,
        cancellation: &CancellationToken,
        progress: &ProgressReporter,
    ) -> Result<TrackAnalysisResult, String>;
}

#[derive(Clone)]
pub struct ProgressReporter {
    callback: Arc<ProgressCallback>,
}

type ProgressCallback = dyn Fn(AnalysisStage, Option<f32>, Option<String>) + Send + Sync;

impl ProgressReporter {
    pub fn noop() -> Self {
        Self {
            callback: Arc::new(|_, _, _| {}),
        }
    }

    pub fn report(&self, stage: AnalysisStage, completed_fraction: Option<f32>) {
        (self.callback)(stage, completed_fraction, None);
    }
}

impl<F> AnalysisProcessor for F
where
    F: Fn(
            &TrackIdentity,
            &CancellationToken,
            &ProgressReporter,
        ) -> Result<TrackAnalysisResult, String>
        + Send
        + Sync
        + 'static,
{
    fn analyze(
        &self,
        track: &TrackIdentity,
        cancellation: &CancellationToken,
        progress: &ProgressReporter,
    ) -> Result<TrackAnalysisResult, String> {
        self(track, cancellation, progress)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisJobSnapshot {
    pub track_id: i64,
    pub stage: AnalysisStage,
    pub completed_fraction: Option<f32>,
    pub queue_position: Option<usize>,
    pub message: Option<String>,
}

struct QueuedJob {
    track: TrackIdentity,
    cancellation: CancellationToken,
}

struct ActiveJob {
    track: TrackIdentity,
    cancellation: CancellationToken,
}

struct State {
    queue: VecDeque<QueuedJob>,
    active: Option<ActiveJob>,
    snapshots: HashMap<i64, AnalysisJobSnapshot>,
    shutting_down: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
    capacity: usize,
}

pub struct AnalysisService {
    shared: Arc<Shared>,
    persistence: Arc<PersistenceWorker>,
    join: Option<JoinHandle<()>>,
}

impl AnalysisService {
    pub fn start(
        persistence: Arc<PersistenceWorker>,
        processor: impl AnalysisProcessor,
    ) -> Result<Self, AnalysisServiceError> {
        Self::start_with_capacity(persistence, processor, DEFAULT_ANALYSIS_QUEUE_CAPACITY)
    }

    pub fn start_with_capacity(
        persistence: Arc<PersistenceWorker>,
        processor: impl AnalysisProcessor,
        capacity: usize,
    ) -> Result<Self, AnalysisServiceError> {
        if capacity == 0 {
            return Err(AnalysisServiceError::InvalidCapacity);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue: VecDeque::with_capacity(capacity),
                active: None,
                snapshots: HashMap::new(),
                shutting_down: false,
            }),
            wake: Condvar::new(),
            capacity,
        });
        let worker_shared = Arc::clone(&shared);
        let worker_persistence = Arc::clone(&persistence);
        let processor = Arc::new(processor);
        let join = thread::Builder::new()
            .name("djapp-analysis".to_string())
            .spawn(move || run_worker(worker_shared, worker_persistence, processor))
            .map_err(|_| AnalysisServiceError::WorkerUnavailable)?;
        Ok(Self {
            shared,
            persistence,
            join: Some(join),
        })
    }

    pub fn enqueue(&self, track: TrackIdentity) -> Result<(), AnalysisServiceError> {
        validate_identity(&track)?;
        let mut state = self.shared.state.lock().map_err(lock_error)?;
        if state.shutting_down {
            return Err(AnalysisServiceError::ShuttingDown);
        }
        if state.active.as_ref().is_some_and(|job| job.track == track)
            || state.queue.iter().any(|job| job.track == track)
        {
            return Err(AnalysisServiceError::AlreadyQueued);
        }

        let replaced_queued = state
            .queue
            .iter()
            .any(|job| job.track.track_id == track.track_id);
        if !replaced_queued && state.queue.len() >= self.shared.capacity {
            return Err(AnalysisServiceError::QueueFull);
        }

        persist_status(&self.persistence, track.track_id, "pending", None, None)?;
        if replaced_queued {
            state
                .queue
                .retain(|job| job.track.track_id != track.track_id);
        }
        if let Some(active) = state
            .active
            .as_ref()
            .filter(|job| job.track.track_id == track.track_id)
        {
            active.cancellation.cancel();
        }
        state.queue.push_back(QueuedJob {
            cancellation: CancellationToken::new(),
            track: track.clone(),
        });
        state.snapshots.insert(
            track.track_id,
            AnalysisJobSnapshot {
                track_id: track.track_id,
                stage: AnalysisStage::Queued,
                completed_fraction: Some(0.0),
                queue_position: None,
                message: replaced_queued.then(|| "replaced stale queued analysis".to_string()),
            },
        );
        drop(state);
        self.shared.wake.notify_one();
        Ok(())
    }

    pub fn cancel(&self, track_id: i64) -> Result<bool, AnalysisServiceError> {
        let mut state = self.shared.state.lock().map_err(lock_error)?;
        let before = state.queue.len();
        state.queue.retain(|job| job.track.track_id != track_id);
        let removed_queued = state.queue.len() != before;
        let cancelled_active = state
            .active
            .as_ref()
            .filter(|job| job.track.track_id == track_id)
            .map(|job| {
                job.cancellation.cancel();
                true
            })
            .unwrap_or(false);
        if !removed_queued && !cancelled_active {
            return Ok(false);
        }
        state.snapshots.insert(
            track_id,
            AnalysisJobSnapshot {
                track_id,
                stage: AnalysisStage::Cancelled,
                completed_fraction: None,
                queue_position: None,
                message: Some(CANCELLED_MESSAGE.to_string()),
            },
        );
        if removed_queued {
            persist_status(
                &self.persistence,
                track_id,
                "failed",
                Some(CANCELLED_MESSAGE),
                Some(now_ms()),
            )?;
        }
        Ok(true)
    }

    pub fn snapshots(&self) -> Result<Vec<AnalysisJobSnapshot>, AnalysisServiceError> {
        let state = self.shared.state.lock().map_err(lock_error)?;
        let mut snapshots: Vec<_> = state.snapshots.values().cloned().collect();
        for snapshot in &mut snapshots {
            snapshot.queue_position = None;
            if state
                .active
                .as_ref()
                .is_some_and(|job| job.track.track_id == snapshot.track_id)
            {
                snapshot.queue_position = Some(0);
            } else if let Some((position, _)) = state
                .queue
                .iter()
                .enumerate()
                .find(|(_, job)| job.track.track_id == snapshot.track_id)
            {
                snapshot.queue_position = Some(position + 1);
            }
        }
        snapshots.sort_by_key(|snapshot| {
            (
                snapshot.queue_position.unwrap_or(usize::MAX),
                snapshot.track_id,
            )
        });
        Ok(snapshots)
    }

    pub fn shutdown(mut self) -> Result<(), AnalysisServiceError> {
        self.signal_shutdown()?;
        self.join_worker()
    }

    fn signal_shutdown(&self) -> Result<(), AnalysisServiceError> {
        let mut state = self.shared.state.lock().map_err(lock_error)?;
        state.shutting_down = true;
        if let Some(active) = &state.active {
            active.cancellation.cancel();
        }
        for job in &state.queue {
            job.cancellation.cancel();
        }
        drop(state);
        self.shared.wake.notify_all();
        Ok(())
    }

    fn join_worker(&mut self) -> Result<(), AnalysisServiceError> {
        self.join
            .take()
            .expect("analysis worker join handle must exist")
            .join()
            .map_err(|_| AnalysisServiceError::WorkerPanicked)
    }
}

impl Drop for AnalysisService {
    fn drop(&mut self) {
        let _ = self.signal_shutdown();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(
    shared: Arc<Shared>,
    persistence: Arc<PersistenceWorker>,
    processor: Arc<impl AnalysisProcessor>,
) {
    loop {
        let job = {
            let mut state = match shared.state.lock() {
                Ok(state) => state,
                Err(_) => return,
            };
            while state.queue.is_empty() && !state.shutting_down {
                state = match shared.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
            }
            if state.shutting_down {
                let cancelled: Vec<_> = state.queue.drain(..).collect();
                for job in cancelled {
                    state.snapshots.insert(
                        job.track.track_id,
                        AnalysisJobSnapshot {
                            track_id: job.track.track_id,
                            stage: AnalysisStage::Cancelled,
                            completed_fraction: None,
                            queue_position: None,
                            message: Some(CANCELLED_MESSAGE.to_string()),
                        },
                    );
                    let _ = persist_status(
                        &persistence,
                        job.track.track_id,
                        "failed",
                        Some(CANCELLED_MESSAGE),
                        Some(now_ms()),
                    );
                }
                return;
            }
            let job = state
                .queue
                .pop_front()
                .expect("analysis queue is not empty");
            state.active = Some(ActiveJob {
                track: job.track.clone(),
                cancellation: job.cancellation.clone(),
            });
            state.snapshots.insert(
                job.track.track_id,
                AnalysisJobSnapshot {
                    track_id: job.track.track_id,
                    stage: AnalysisStage::Decoding,
                    completed_fraction: None,
                    queue_position: None,
                    message: None,
                },
            );
            job
        };

        let progress_shared = Arc::clone(&shared);
        let progress_track_id = job.track.track_id;
        let progress = ProgressReporter {
            callback: Arc::new(move |stage, completed_fraction, message| {
                if let Ok(mut state) = progress_shared.state.lock() {
                    state.snapshots.insert(
                        progress_track_id,
                        AnalysisJobSnapshot {
                            track_id: progress_track_id,
                            stage,
                            completed_fraction,
                            queue_position: None,
                            message,
                        },
                    );
                }
            }),
        };
        let outcome = persist_status(&persistence, job.track.track_id, "running", None, None)
            .and_then(|_| {
                processor
                    .analyze(&job.track, &job.cancellation, &progress)
                    .map_err(AnalysisServiceError::Processing)
            });
        let cancelled = job.cancellation.is_cancelled();
        let replacement_pending = cancelled
            && shared.state.lock().is_ok_and(|state| {
                state
                    .queue
                    .iter()
                    .any(|queued| queued.track.track_id == job.track.track_id)
            });
        let (stage, message) = if cancelled {
            if !replacement_pending {
                let _ = persist_status(
                    &persistence,
                    job.track.track_id,
                    "failed",
                    Some(CANCELLED_MESSAGE),
                    Some(now_ms()),
                );
            }
            (
                AnalysisStage::Cancelled,
                Some(CANCELLED_MESSAGE.to_string()),
            )
        } else {
            match outcome {
                Ok(result) => match persist_complete(&persistence, job.track.track_id, result) {
                    Ok(()) => (AnalysisStage::Complete, None),
                    Err(error) => {
                        let message = error.to_string();
                        let _ = persist_status(
                            &persistence,
                            job.track.track_id,
                            "failed",
                            Some(&message),
                            Some(now_ms()),
                        );
                        (AnalysisStage::Failed, Some(message))
                    }
                },
                Err(error) => {
                    let message = error.to_string();
                    let _ = persist_status(
                        &persistence,
                        job.track.track_id,
                        "failed",
                        Some(&message),
                        Some(now_ms()),
                    );
                    (AnalysisStage::Failed, Some(message))
                }
            }
        };

        let mut state = match shared.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };
        state.active = None;
        if !replacement_pending {
            state.snapshots.insert(
                job.track.track_id,
                AnalysisJobSnapshot {
                    track_id: job.track.track_id,
                    stage,
                    completed_fraction: (stage == AnalysisStage::Complete).then_some(1.0),
                    queue_position: None,
                    message,
                },
            );
        }
    }
}

fn persist_complete(
    persistence: &PersistenceWorker,
    track_id: i64,
    result: TrackAnalysisResult,
) -> Result<(), AnalysisServiceError> {
    if result.analysis_version != ANALYSIS_VERSION
        || result.bpm.is_some_and(|estimate| {
            !estimate.value.is_finite()
                || estimate.value <= 0.0
                || !estimate.confidence.is_finite()
                || !(0.0..=1.0).contains(&estimate.confidence)
        })
        || result.musical_key.is_some_and(|estimate| {
            estimate.value.pitch_class >= 12
                || !estimate.confidence.is_finite()
                || !(0.0..=1.0).contains(&estimate.confidence)
        })
        || result
            .integrated_lufs
            .is_some_and(|value| !value.is_finite())
        || result
            .true_peak_dbtp
            .is_some_and(|value| !value.is_finite())
    {
        return Err(AnalysisServiceError::InvalidResult);
    }
    let musical_key = result.musical_key.map(|estimate| {
        let mode = match estimate.value.mode {
            MusicalMode::Major => "major",
            MusicalMode::Minor => "minor",
        };
        format!("{}:{mode}", estimate.value.pitch_class)
    });
    persistence
        .save_analysis(AnalysisRecord {
            track_id,
            analysis_version: i64::from(result.analysis_version),
            status: "complete".to_string(),
            bpm: result.bpm.map(|estimate| estimate.value),
            bpm_confidence: result.bpm.map(|estimate| f64::from(estimate.confidence)),
            musical_key,
            key_confidence: result
                .musical_key
                .map(|estimate| f64::from(estimate.confidence)),
            integrated_lufs: result.integrated_lufs,
            true_peak_db: result.true_peak_dbtp,
            beat_grid_path: result
                .beat_grid_path
                .map(|path| path.to_string_lossy().into_owned()),
            waveform_path: result
                .waveform_path
                .map(|path| path.to_string_lossy().into_owned()),
            error_message: None,
            analyzed_at_ms: Some(now_ms()),
        })
        .map_err(|error| AnalysisServiceError::Persistence(error.to_string()))
}

fn persist_status(
    persistence: &PersistenceWorker,
    track_id: i64,
    status: &str,
    error_message: Option<&str>,
    analyzed_at_ms: Option<i64>,
) -> Result<(), AnalysisServiceError> {
    persistence
        .save_analysis(AnalysisRecord {
            track_id,
            analysis_version: i64::from(ANALYSIS_VERSION),
            status: status.to_string(),
            bpm: None,
            bpm_confidence: None,
            musical_key: None,
            key_confidence: None,
            integrated_lufs: None,
            true_peak_db: None,
            beat_grid_path: None,
            waveform_path: None,
            error_message: error_message.map(str::to_string),
            analyzed_at_ms,
        })
        .map_err(|error| AnalysisServiceError::Persistence(error.to_string()))
}

fn validate_identity(track: &TrackIdentity) -> Result<(), AnalysisServiceError> {
    if track.track_id <= 0 || track.file_size == 0 || track.path.as_os_str().is_empty() {
        return Err(AnalysisServiceError::InvalidTrack);
    }
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> AnalysisServiceError {
    AnalysisServiceError::WorkerPanicked
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnalysisServiceError {
    InvalidCapacity,
    InvalidTrack,
    InvalidResult,
    AlreadyQueued,
    QueueFull,
    ShuttingDown,
    WorkerUnavailable,
    WorkerPanicked,
    Persistence(String),
    Processing(String),
}

impl fmt::Display for AnalysisServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidCapacity => {
                formatter.write_str("analysis queue capacity must be positive")
            }
            Self::InvalidTrack => formatter.write_str("analysis track identity is invalid"),
            Self::InvalidResult => formatter.write_str("analysis result version is invalid"),
            Self::AlreadyQueued => formatter.write_str("the same track analysis is already queued"),
            Self::QueueFull => formatter.write_str("analysis queue is full"),
            Self::ShuttingDown => formatter.write_str("analysis service is shutting down"),
            Self::WorkerUnavailable => formatter.write_str("analysis worker is unavailable"),
            Self::WorkerPanicked => formatter.write_str("analysis worker panicked"),
            Self::Persistence(error) => write!(formatter, "analysis persistence failed: {error}"),
            Self::Processing(error) => write!(formatter, "analysis failed: {error}"),
        }
    }
}

impl Error for AnalysisServiceError {}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Mutex,
        },
        time::{Duration, Instant},
    };

    use crate::{
        analysis::types::{Estimate, MusicalKey, MusicalMode, TrackAnalysisResult},
        persistence::{NewTrack, PersistenceWorker},
    };

    use super::*;

    fn temporary_database(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "djapp-analysis-{name}-{}-{nonce}.sqlite",
            std::process::id()
        ))
    }

    fn insert_track(persistence: &PersistenceWorker, path: &str, size: i64) -> TrackIdentity {
        let track = persistence
            .upsert_track(NewTrack {
                library_root_id: None,
                path: path.to_string(),
                file_size: size,
                modified_at_ms: 10,
                content_fingerprint: None,
                title: None,
                artist: None,
                album: None,
                genre: None,
                duration_frames: Some(48_000),
                sample_rate: Some(48_000),
                channels: Some(2),
                codec: Some("wav".to_string()),
                missing: false,
                updated_at_ms: 10,
            })
            .unwrap();
        TrackIdentity {
            track_id: track.id,
            path: PathBuf::from(path),
            file_size: size as u64,
            modified_at_ms: 10,
            content_fingerprint: None,
        }
    }

    fn empty_result() -> TrackAnalysisResult {
        TrackAnalysisResult {
            analysis_version: ANALYSIS_VERSION,
            bpm: None,
            musical_key: None,
            integrated_lufs: None,
            true_peak_dbtp: None,
            waveform_path: None,
            beat_grid_path: None,
        }
    }

    fn wait_for_stage(service: &AnalysisService, track_id: i64, stage: AnalysisStage) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if service
                .snapshots()
                .unwrap()
                .iter()
                .any(|snapshot| snapshot.track_id == track_id && snapshot.stage == stage)
            {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("track {track_id} did not reach {stage:?}");
    }

    #[test]
    fn worker_persists_pending_running_and_complete() {
        let path = temporary_database("complete");
        let persistence = Arc::new(PersistenceWorker::start(path.clone()).unwrap());
        let track = insert_track(&persistence, "/music/complete.wav", 100);
        let worker_persistence = Arc::clone(&persistence);
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            move |track: &TrackIdentity, _: &CancellationToken, _: &ProgressReporter| {
                assert_eq!(
                    worker_persistence
                        .analysis(track.track_id)
                        .unwrap()
                        .unwrap()
                        .status,
                    "running"
                );
                let mut result = empty_result();
                result.bpm = Estimate::new(120.0, 0.8);
                result.musical_key =
                    Estimate::new(MusicalKey::new(9, MusicalMode::Minor).unwrap(), 0.7);
                Ok(result)
            },
        )
        .unwrap();
        service.enqueue(track.clone()).unwrap();
        wait_for_stage(&service, track.track_id, AnalysisStage::Complete);
        let record = persistence.analysis(track.track_id).unwrap().unwrap();
        assert_eq!(record.status, "complete");
        assert_eq!(record.analysis_version, i64::from(ANALYSIS_VERSION));
        assert_eq!(record.bpm, Some(120.0));
        assert_eq!(record.bpm_confidence, Some(f64::from(0.8_f32)));
        assert_eq!(record.musical_key.as_deref(), Some("9:minor"));
        assert_eq!(record.key_confidence, Some(f64::from(0.7_f32)));
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn duplicate_identity_is_rejected() {
        let path = temporary_database("duplicate");
        let persistence = Arc::new(PersistenceWorker::start(path.clone()).unwrap());
        let track = insert_track(&persistence, "/music/duplicate.wav", 100);
        let release = Arc::new(AtomicBool::new(false));
        let worker_release = Arc::clone(&release);
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            move |_: &TrackIdentity, cancellation: &CancellationToken, _: &ProgressReporter| {
                while !worker_release.load(Ordering::Acquire) && !cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(empty_result())
            },
        )
        .unwrap();
        service.enqueue(track.clone()).unwrap();
        assert_eq!(
            service.enqueue(track.clone()),
            Err(AnalysisServiceError::AlreadyQueued)
        );
        release.store(true, Ordering::Release);
        wait_for_stage(&service, track.track_id, AnalysisStage::Complete);
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn changed_queued_identity_replaces_stale_work() {
        let path = temporary_database("replace");
        let persistence = Arc::new(PersistenceWorker::start(path.clone()).unwrap());
        let first = insert_track(&persistence, "/music/blocking.wav", 100);
        let stale = insert_track(&persistence, "/music/replaced.wav", 101);
        let mut current = stale.clone();
        current.file_size = 202;
        current.modified_at_ms = 20;
        let release = Arc::new(AtomicBool::new(false));
        let worker_release = Arc::clone(&release);
        let processed_sizes = Arc::new(Mutex::new(Vec::new()));
        let worker_sizes = Arc::clone(&processed_sizes);
        let blocking_id = first.track_id;
        let service = AnalysisService::start_with_capacity(
            Arc::clone(&persistence),
            move |track: &TrackIdentity, cancellation: &CancellationToken, _: &ProgressReporter| {
                if track.track_id == blocking_id {
                    while !worker_release.load(Ordering::Acquire) && !cancellation.is_cancelled() {
                        thread::sleep(Duration::from_millis(1));
                    }
                }
                worker_sizes.lock().unwrap().push(track.file_size);
                Ok(empty_result())
            },
            2,
        )
        .unwrap();
        service.enqueue(first.clone()).unwrap();
        wait_for_stage(&service, first.track_id, AnalysisStage::Decoding);
        service.enqueue(stale).unwrap();
        service.enqueue(current.clone()).unwrap();
        release.store(true, Ordering::Release);
        wait_for_stage(&service, current.track_id, AnalysisStage::Complete);
        assert_eq!(*processed_sizes.lock().unwrap(), vec![100, 202]);
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn active_job_cancellation_is_cooperative_and_persisted() {
        let path = temporary_database("cancel");
        let persistence = Arc::new(PersistenceWorker::start(path.clone()).unwrap());
        let track = insert_track(&persistence, "/music/cancel.wav", 100);
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            |_: &TrackIdentity, cancellation: &CancellationToken, _: &ProgressReporter| {
                while !cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(empty_result())
            },
        )
        .unwrap();
        service.enqueue(track.clone()).unwrap();
        wait_for_stage(&service, track.track_id, AnalysisStage::Decoding);
        assert!(service.cancel(track.track_id).unwrap());
        wait_for_stage(&service, track.track_id, AnalysisStage::Cancelled);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let record = persistence.analysis(track.track_id).unwrap().unwrap();
            if record.status == "failed" {
                assert_eq!(record.error_message.as_deref(), Some(CANCELLED_MESSAGE));
                break;
            }
            assert!(Instant::now() < deadline, "cancellation was not persisted");
            thread::sleep(Duration::from_millis(5));
        }
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bounded_queue_rejects_excess_work_and_shutdown_cancels_pending() {
        let path = temporary_database("bounded");
        let persistence = Arc::new(PersistenceWorker::start(path.clone()).unwrap());
        let first = insert_track(&persistence, "/music/first.wav", 100);
        let second = insert_track(&persistence, "/music/second.wav", 101);
        let third = insert_track(&persistence, "/music/third.wav", 102);
        let started = Arc::new(AtomicUsize::new(0));
        let worker_started = Arc::clone(&started);
        let service = AnalysisService::start_with_capacity(
            Arc::clone(&persistence),
            move |_: &TrackIdentity, cancellation: &CancellationToken, _: &ProgressReporter| {
                worker_started.fetch_add(1, Ordering::Relaxed);
                while !cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(1));
                }
                Ok(empty_result())
            },
            1,
        )
        .unwrap();
        service.enqueue(first.clone()).unwrap();
        wait_for_stage(&service, first.track_id, AnalysisStage::Decoding);
        service.enqueue(second.clone()).unwrap();
        assert_eq!(service.enqueue(third), Err(AnalysisServiceError::QueueFull));
        service.shutdown().unwrap();
        assert_eq!(started.load(Ordering::Relaxed), 1);
        assert_eq!(
            persistence
                .analysis(second.track_id)
                .unwrap()
                .unwrap()
                .status,
            "failed"
        );
        drop(persistence);
        fs::remove_file(path).unwrap();
    }
}
