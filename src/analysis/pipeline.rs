use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use ebur128::{EbuR128, Mode};

use crate::media::decode::MediaDecoder;

use super::{
    cache::{BeatGridCache, BeatRecord},
    key::KeyAnalyzer,
    rhythm::RhythmAnalyzer,
    service::{AnalysisProcessor, CancellationToken, ProgressReporter},
    signal::{analysis_frame_to_source_frame, AnalysisSignalBuilder},
    types::{TrackAnalysisResult, TrackIdentity},
    waveform::WaveformBuilder,
    ANALYSIS_VERSION,
};

pub struct WaveformLoudnessProcessor {
    cache_root: PathBuf,
}

impl WaveformLoudnessProcessor {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
        }
    }
}

impl AnalysisProcessor for WaveformLoudnessProcessor {
    fn analyze(
        &self,
        track: &TrackIdentity,
        cancellation: &CancellationToken,
        progress: &ProgressReporter,
    ) -> Result<TrackAnalysisResult, String> {
        validate_file_identity(track)?;
        if cancellation.is_cancelled() {
            return Err("analysis cancelled".to_string());
        }

        let mut decoder = MediaDecoder::open(&track.path).map_err(|error| error.to_string())?;
        let info = decoder.info().clone();
        let channels = u32::try_from(info.channels)
            .map_err(|_| "audio has too many channels for loudness analysis".to_string())?;
        let mut loudness = EbuR128::new(channels, info.sample_rate, Mode::I | Mode::TRUE_PEAK)
            .map_err(|error| format!("cannot initialize loudness analysis: {error}"))?;
        let identity_digest = identity_digest(track);
        let mut waveform = WaveformBuilder::new(info.channels)?;
        let mut analysis_signal = AnalysisSignalBuilder::new(info.sample_rate, info.channels)?;
        let mut rhythm = RhythmAnalyzer::new();
        let mut key = KeyAnalyzer::new();
        let mut decode_buffer = Vec::new();
        progress.report(super::types::AnalysisStage::Waveform, Some(0.1));

        while let Some(chunk) = decoder
            .next_chunk_into(std::mem::take(&mut decode_buffer))
            .map_err(|error| error.to_string())?
        {
            if cancellation.is_cancelled() {
                return Err("analysis cancelled".to_string());
            }
            if chunk.sample_rate != info.sample_rate || chunk.channels != info.channels {
                return Err("audio format changed during analysis".to_string());
            }
            waveform.push_interleaved(&chunk.samples)?;
            analysis_signal.push_interleaved(&chunk.samples)?;
            let fixed_rate = analysis_signal.take_output();
            rhythm.push_signal(&fixed_rate)?;
            key.push_signal(&fixed_rate)?;
            loudness
                .add_frames_f32(&chunk.samples)
                .map_err(|error| format!("loudness analysis failed: {error}"))?;
            decode_buffer = chunk.samples;
        }

        if cancellation.is_cancelled() {
            return Err("analysis cancelled".to_string());
        }
        validate_file_identity(track)?;
        progress.report(super::types::AnalysisStage::Rhythm, Some(0.65));
        let waveform = waveform.finish(identity_digest, info.sample_rate)?;
        let fixed_rate = analysis_signal.finish()?;
        rhythm.push_signal(&fixed_rate)?;
        key.push_signal(&fixed_rate)?;
        let rhythm = rhythm.finish()?;
        progress.report(super::types::AnalysisStage::Key, Some(0.78));
        let key = key.finish()?;
        progress.report(super::types::AnalysisStage::Loudness, Some(0.88));
        let waveform_path = cache_path(
            &self.cache_root,
            track.track_id,
            identity_digest,
            "waveform",
        );
        let beat_grid = rhythm
            .bpm
            .zip(rhythm.beat_grid.as_ref())
            .map(|(bpm, grid)| BeatGridCache {
                identity_digest,
                source_sample_rate: info.sample_rate,
                source_channels: waveform.source_channels,
                source_frames: waveform.source_frames,
                bpm: bpm.value,
                confidence: grid.confidence,
                beats: source_beats(grid, info.sample_rate, waveform.source_frames),
            })
            .filter(|grid| grid.beats.len() >= 4);
        let beat_grid_path = beat_grid.as_ref().map(|_| {
            cache_path(
                &self.cache_root,
                track.track_id,
                identity_digest,
                "beat-grid",
            )
        });
        progress.report(super::types::AnalysisStage::Writing, Some(0.94));
        write_cache_atomic(
            &waveform_path,
            &waveform.encode().map_err(|error| error.to_string())?,
        )?;
        if let (Some(path), Some(grid)) = (&beat_grid_path, &beat_grid) {
            write_cache_atomic(path, &grid.encode().map_err(|error| error.to_string())?)?;
        }

        Ok(TrackAnalysisResult {
            analysis_version: ANALYSIS_VERSION,
            bpm: rhythm.bpm,
            musical_key: key.key,
            integrated_lufs: finite(loudness.loudness_global().ok()),
            true_peak_dbtp: true_peak_dbtp(&loudness, channels),
            waveform_path: Some(waveform_path),
            beat_grid_path,
        })
    }
}

fn validate_file_identity(track: &TrackIdentity) -> Result<(), String> {
    let metadata = fs::metadata(&track.path)
        .map_err(|error| format!("cannot read analysis source: {error}"))?;
    if metadata.len() != track.file_size {
        return Err("analysis source changed size".to_string());
    }
    if track.modified_at_ms > 0 {
        let modified_at_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0);
        if modified_at_ms != track.modified_at_ms {
            return Err("analysis source modification time changed".to_string());
        }
    }
    Ok(())
}

fn true_peak_dbtp(loudness: &EbuR128, channels: u32) -> Option<f64> {
    let peak = (0..channels)
        .filter_map(|channel| loudness.true_peak(channel).ok())
        .filter(|value| value.is_finite())
        .fold(0.0_f64, f64::max);
    (peak > 0.0).then(|| 20.0 * peak.log10())
}

fn finite(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite())
}

fn source_beats(
    grid: &super::rhythm::TrackedBeatGrid,
    source_rate: u32,
    source_frames: u64,
) -> Vec<BeatRecord> {
    let mut beats = Vec::with_capacity(grid.beats.len());
    for beat in &grid.beats {
        let source_frame = analysis_frame_to_source_frame(beat.analysis_frame, source_rate);
        if source_frame >= source_frames
            || beats
                .last()
                .is_some_and(|previous: &BeatRecord| previous.source_frame >= source_frame)
        {
            continue;
        }
        beats.push(BeatRecord {
            source_frame,
            strength: beat.strength,
            downbeat: beat.downbeat,
        });
    }
    beats
}

fn cache_path(cache_root: &Path, track_id: i64, digest: [u8; 32], extension: &str) -> PathBuf {
    let digest = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    cache_root
        .join(format!("analysis-v{ANALYSIS_VERSION}"))
        .join(format!("track-{track_id}-{digest}.{extension}"))
}

fn write_cache_atomic(path: &Path, encoded: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "waveform cache path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("analysis-cache"),
        std::process::id(),
        nonce
    ));
    let result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
        file.write_all(encoded).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        fs::rename(&temporary, path).map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn identity_digest(track: &TrackIdentity) -> [u8; 32] {
    let mut lanes = [
        0xcbf2_9ce4_8422_2325_u64,
        0x8422_2325_cbf2_9ce4_u64,
        0x9e37_79b9_7f4a_7c15_u64,
        0x517c_c1b7_2722_0a95_u64,
    ];
    hash_bytes(&mut lanes, &track.track_id.to_le_bytes());
    hash_bytes(&mut lanes, track.path.to_string_lossy().as_bytes());
    hash_bytes(&mut lanes, &track.file_size.to_le_bytes());
    hash_bytes(&mut lanes, &track.modified_at_ms.to_le_bytes());
    if let Some(fingerprint) = &track.content_fingerprint {
        hash_bytes(&mut lanes, fingerprint.as_bytes());
    }
    let mut digest = [0; 32];
    for (index, lane) in lanes.iter().enumerate() {
        digest[index * 8..(index + 1) * 8].copy_from_slice(&lane.to_le_bytes());
    }
    digest
}

fn hash_bytes(lanes: &mut [u64; 4], bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        let lane = &mut lanes[index % lanes.len()];
        *lane ^= u64::from(*byte);
        *lane = lane.wrapping_mul(0x0000_0100_0000_01b3);
        *lane ^= lane.rotate_right(17);
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use crate::{
        analysis::{
            cache::{BeatGridCache, WaveformCache},
            service::AnalysisService,
            types::AnalysisStage,
        },
        persistence::{NewTrack, PersistenceWorker},
    };

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "djapp-analysis-pipeline-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn identity(path: &Path, track_id: i64) -> TrackIdentity {
        let metadata = fs::metadata(path).unwrap();
        let modified_at_ms = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        TrackIdentity {
            track_id,
            path: path.to_path_buf(),
            file_size: metadata.len(),
            modified_at_ms,
            content_fingerprint: None,
        }
    }

    #[test]
    fn processor_writes_valid_waveform_and_loudness() {
        let root = temp_dir("direct");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        fs::copy("tests/fixtures/audio/tone.wav", &source).unwrap();
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        let token = CancellationToken::new();
        let result = processor
            .analyze(&identity(&source, 1), &token, &ProgressReporter::noop())
            .unwrap();
        assert!(result
            .integrated_lufs
            .is_some_and(|value| value.is_finite()));
        assert!(result.true_peak_dbtp.is_some_and(|value| value <= 0.1));
        let path = result.waveform_path.unwrap();
        let waveform = WaveformCache::decode(&fs::read(path).unwrap()).unwrap();
        assert_eq!(waveform.source_sample_rate, 44_100);
        assert_eq!(waveform.source_channels, 2);
        assert_eq!(waveform.source_frames, 132_300);
        assert!(waveform.levels.len() >= 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn changed_identity_is_rejected_before_cache_write() {
        let root = temp_dir("identity");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        fs::copy("tests/fixtures/audio/tone.wav", &source).unwrap();
        let mut track = identity(&source, 1);
        track.file_size += 1;
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        assert_eq!(
            processor
                .analyze(&track, &CancellationToken::new(), &ProgressReporter::noop(),)
                .unwrap_err(),
            "analysis source changed size"
        );
        assert!(!root.join("cache").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancelled_processor_does_not_create_cache() {
        let root = temp_dir("cancelled");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        fs::copy("tests/fixtures/audio/tone.wav", &source).unwrap();
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        assert_eq!(
            processor
                .analyze(
                    &identity(&source, 1),
                    &cancellation,
                    &ProgressReporter::noop(),
                )
                .unwrap_err(),
            "analysis cancelled"
        );
        assert!(!root.join("cache").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn processor_supports_all_decoder_fixture_formats() {
        let root = temp_dir("formats");
        fs::create_dir_all(&root).unwrap();
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        for (index, fixture) in [
            "tests/fixtures/audio/tone.mp3",
            "tests/fixtures/audio/tone.wav",
            "tests/fixtures/audio/tone.flac",
            "tests/fixtures/audio/tone.m4a",
            "tests/fixtures/audio/tone.aiff",
        ]
        .iter()
        .enumerate()
        {
            let extension = Path::new(fixture).extension().unwrap().to_string_lossy();
            let source = root.join(format!("tone-{index}.{extension}"));
            fs::copy(fixture, &source).unwrap();
            let result = processor
                .analyze(
                    &identity(&source, index as i64 + 1),
                    &CancellationToken::new(),
                    &ProgressReporter::noop(),
                )
                .unwrap();
            assert!(result.integrated_lufs.is_some());
            assert!(result.true_peak_dbtp.is_some());
            assert!(result.waveform_path.unwrap().is_file());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn music_like_fixture_produces_expected_bpm() {
        let root = temp_dir("music-bpm");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("music.wav");
        fs::copy("tests/fixtures/audio/music-like-48k.wav", &source).unwrap();
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        let result = processor
            .analyze(
                &identity(&source, 1),
                &CancellationToken::new(),
                &ProgressReporter::noop(),
            )
            .unwrap();
        let bpm = result.bpm.unwrap();
        assert!(
            (bpm.value - 120.0).abs() / 120.0 <= 0.01,
            "measured BPM was {}",
            bpm.value
        );
        assert!(bpm.confidence >= 0.53);
        let beat_grid_path = result.beat_grid_path.unwrap();
        let beat_grid = BeatGridCache::decode(&fs::read(beat_grid_path).unwrap()).unwrap();
        assert!(beat_grid.confidence >= 0.65);
        assert!(beat_grid.beats.len() >= 20);
        assert!(beat_grid
            .beats
            .windows(2)
            .all(|pair| pair[0].source_frame < pair[1].source_frame));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn steady_tone_does_not_claim_a_bpm() {
        let root = temp_dir("no-bpm");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("tone.wav");
        fs::copy("tests/fixtures/audio/tone.wav", &source).unwrap();
        let processor = WaveformLoudnessProcessor::new(root.join("cache"));
        let result = processor
            .analyze(
                &identity(&source, 1),
                &CancellationToken::new(),
                &ProgressReporter::noop(),
            )
            .unwrap();
        assert!(result.bpm.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn service_persists_waveform_loudness_and_beat_grid_result() {
        let root = temp_dir("service");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("music.wav");
        fs::copy("tests/fixtures/audio/music-like-48k.wav", &source).unwrap();
        let persistence = Arc::new(PersistenceWorker::start(root.join("library.sqlite")).unwrap());
        let metadata = fs::metadata(&source).unwrap();
        let modified_at_ms = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let track_id = persistence
            .upsert_track(NewTrack {
                library_root_id: None,
                path: source.to_string_lossy().into_owned(),
                file_size: metadata.len() as i64,
                modified_at_ms,
                content_fingerprint: None,
                title: Some("Music fixture".to_string()),
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
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            WaveformLoudnessProcessor::new(root.join("cache")),
        )
        .unwrap();
        service.enqueue(identity(&source, track_id)).unwrap();
        let deadline = SystemTime::now() + std::time::Duration::from_secs(30);
        loop {
            if service.snapshots().unwrap().iter().any(|snapshot| {
                snapshot.track_id == track_id && snapshot.stage == AnalysisStage::Complete
            }) {
                break;
            }
            assert!(SystemTime::now() < deadline, "analysis did not complete");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let record = persistence.analysis(track_id).unwrap().unwrap();
        assert_eq!(record.status, "complete");
        assert!(record.integrated_lufs.is_some());
        assert!(record.true_peak_db.is_some());
        assert!(record.bpm.is_some_and(|bpm| (bpm - 120.0).abs() < 1.2));
        assert!(record
            .bpm_confidence
            .is_some_and(|confidence| confidence >= 0.53));
        assert!(record
            .beat_grid_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file()));
        assert!(record
            .waveform_path
            .as_deref()
            .is_some_and(|path| Path::new(path).is_file()));
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn completed_caches_reopen_without_decoding_source_again() {
        let root = temp_dir("cache-reopen");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("music.wav");
        fs::copy("tests/fixtures/audio/music-like-48k.wav", &source).unwrap();
        let database_path = root.join("library.sqlite");
        let persistence = Arc::new(PersistenceWorker::start(database_path.clone()).unwrap());
        let metadata = fs::metadata(&source).unwrap();
        let modified_at_ms = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let track_id = persistence
            .upsert_track(NewTrack {
                library_root_id: None,
                path: source.to_string_lossy().into_owned(),
                file_size: metadata.len() as i64,
                modified_at_ms,
                content_fingerprint: None,
                title: Some("Cache reopen".to_string()),
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
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            WaveformLoudnessProcessor::new(root.join("cache")),
        )
        .unwrap();
        service.enqueue(identity(&source, track_id)).unwrap();
        let deadline = SystemTime::now() + std::time::Duration::from_secs(30);
        while !service.snapshots().unwrap().iter().any(|snapshot| {
            snapshot.track_id == track_id && snapshot.stage == AnalysisStage::Complete
        }) {
            assert!(SystemTime::now() < deadline, "analysis did not complete");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_file(&source).unwrap();

        let reopened = PersistenceWorker::start(database_path).unwrap();
        let record = reopened.analysis(track_id).unwrap().unwrap();
        let waveform = WaveformCache::decode(
            &fs::read(record.waveform_path.expect("waveform path persisted")).unwrap(),
        )
        .unwrap();
        let grid = BeatGridCache::decode(
            &fs::read(record.beat_grid_path.expect("beat-grid path persisted")).unwrap(),
        )
        .unwrap();
        assert_eq!(waveform.source_frames, grid.source_frames);
        assert_eq!(waveform.identity_digest, grid.identity_digest);
        reopened.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_production_analysis_cancels_without_publishing_partial_caches() {
        let root = temp_dir("active-cancel");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("music.wav");
        fs::copy("tests/fixtures/audio/music-like-48k.wav", &source).unwrap();
        let persistence = Arc::new(PersistenceWorker::start(root.join("library.sqlite")).unwrap());
        let metadata = fs::metadata(&source).unwrap();
        let modified_at_ms = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let track_id = persistence
            .upsert_track(NewTrack {
                library_root_id: None,
                path: source.to_string_lossy().into_owned(),
                file_size: metadata.len() as i64,
                modified_at_ms,
                content_fingerprint: None,
                title: Some("Cancelled analysis".to_string()),
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
        let cache_root = root.join("cache");
        let service = AnalysisService::start(
            Arc::clone(&persistence),
            WaveformLoudnessProcessor::new(&cache_root),
        )
        .unwrap();
        service.enqueue(identity(&source, track_id)).unwrap();
        let deadline = SystemTime::now() + std::time::Duration::from_secs(5);
        while !service.snapshots().unwrap().iter().any(|snapshot| {
            snapshot.track_id == track_id && snapshot.stage == AnalysisStage::Waveform
        }) {
            assert!(
                SystemTime::now() < deadline,
                "analysis did not become active"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(service.cancel(track_id).unwrap());
        while !service.snapshots().unwrap().iter().any(|snapshot| {
            snapshot.track_id == track_id && snapshot.stage == AnalysisStage::Cancelled
        }) {
            assert!(SystemTime::now() < deadline, "analysis did not cancel");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let record = loop {
            let record = persistence.analysis(track_id).unwrap().unwrap();
            if record.status == "failed" {
                break record;
            }
            assert!(
                SystemTime::now() < deadline,
                "cancelled status was not persisted"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(record.status, "failed");
        assert_eq!(record.error_message.as_deref(), Some("analysis cancelled"));
        assert_eq!(record.waveform_path, None);
        assert_eq!(record.beat_grid_path, None);
        assert!(!cache_root.exists());
        service.shutdown().unwrap();
        drop(persistence);
        fs::remove_dir_all(root).unwrap();
    }
}
