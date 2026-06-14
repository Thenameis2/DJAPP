use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    media::decode::MediaDecoder,
    persistence::{NewTrack, PersistenceWorker, TrackChange},
};

const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "wav", "flac", "aac", "m4a", "aif", "aiff"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    pub library_root_id: i64,
    pub root_path: String,
    pub discovered: usize,
    pub inserted: usize,
    pub modified: usize,
    pub restored: usize,
    pub unchanged: usize,
    pub missing: usize,
    pub metadata_failures: usize,
    pub traversal_errors: Vec<String>,
}

pub fn scan_library_root(
    root: impl AsRef<Path>,
    persistence: &PersistenceWorker,
) -> Result<ScanSummary, String> {
    let root = root
        .as_ref()
        .canonicalize()
        .map_err(|error| format!("cannot open selected folder: {error}"))?;
    if !root.is_dir() {
        return Err("selected path is not a folder".to_string());
    }

    let root_path = display_path(&root);
    let display_name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    let scanned_at_ms = unix_time_ms()?;
    let library_root_id = persistence
        .add_library_root(root_path.clone(), display_name, scanned_at_ms)
        .map_err(|error| error.to_string())?;

    let mut files = Vec::new();
    let mut traversal_errors = Vec::new();
    collect_supported_files(&root, &mut files, &mut traversal_errors);
    files.sort();
    files.dedup();

    let mut summary = ScanSummary {
        library_root_id,
        root_path,
        discovered: files.len(),
        inserted: 0,
        modified: 0,
        restored: 0,
        unchanged: 0,
        missing: 0,
        metadata_failures: 0,
        traversal_errors,
    };
    let mut seen_paths = HashSet::with_capacity(files.len());

    for path in files {
        let path_text = display_path(&path);
        seen_paths.insert(path_text.clone());
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                summary
                    .traversal_errors
                    .push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        let modified_at_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or(0);
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        let fallback_title = path
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned());

        let (title, artist, duration_frames, sample_rate, channels, codec) =
            match MediaDecoder::open(&path) {
                Ok(decoder) => {
                    let info = decoder.info();
                    let duration_frames = info.duration_seconds.map(|seconds| {
                        (seconds * f64::from(info.sample_rate))
                            .round()
                            .min(i64::MAX as f64) as i64
                    });
                    (
                        info.title.clone().or(fallback_title),
                        info.artist.clone(),
                        duration_frames,
                        Some(i64::from(info.sample_rate)),
                        Some(info.channels.min(i64::MAX as usize) as i64),
                        Some(info.codec.clone()),
                    )
                }
                Err(_) => {
                    summary.metadata_failures += 1;
                    (fallback_title, None, None, None, None, extension)
                }
            };

        let change = persistence
            .upsert_track(NewTrack {
                library_root_id: Some(library_root_id),
                path: path_text,
                file_size: metadata.len().min(i64::MAX as u64) as i64,
                modified_at_ms,
                content_fingerprint: None,
                title,
                artist,
                album: None,
                genre: None,
                duration_frames,
                sample_rate,
                channels,
                codec,
                missing: false,
                updated_at_ms: scanned_at_ms,
            })
            .map_err(|error| error.to_string())?
            .change;
        match change {
            TrackChange::Inserted => summary.inserted += 1,
            TrackChange::Modified => summary.modified += 1,
            TrackChange::Restored => summary.restored += 1,
            TrackChange::Unchanged => summary.unchanged += 1,
        }
    }

    if summary.traversal_errors.is_empty() {
        summary.missing = persistence
            .reconcile_library_root(
                library_root_id,
                seen_paths.into_iter().collect(),
                scanned_at_ms,
            )
            .map_err(|error| error.to_string())?;
    }

    Ok(summary)
}

fn collect_supported_files(directory: &Path, files: &mut Vec<PathBuf>, errors: &mut Vec<String>) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("{}: {error}", directory.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                errors.push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                errors.push(format!("{}: {error}", entry.path().display()));
                continue;
            }
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_supported_files(&entry.path(), files, errors);
        } else if file_type.is_file() && is_supported_audio(&entry.path()) {
            files.push(entry.path());
        }
    }
}

fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .is_some_and(|extension| SUPPORTED_EXTENSIONS.contains(&extension.as_str()))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn unix_time_ms() -> Result<i64, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before Unix epoch: {error}"))?
        .as_millis();
    Ok(millis.min(i64::MAX as u128) as i64)
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "djapp-library-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn scans_nested_supported_files_and_ignores_other_extensions() {
        let root = temp_path("nested");
        let nested = root.join("House").join("Deep");
        fs::create_dir_all(&nested).unwrap();
        fs::copy("tests/fixtures/audio/tone.mp3", root.join("one.MP3")).unwrap();
        fs::copy("tests/fixtures/audio/tone.flac", nested.join("two.flac")).unwrap();
        fs::write(root.join("notes.txt"), b"not audio").unwrap();
        let database = root.join("library.sqlite");
        let persistence = PersistenceWorker::start(database).unwrap();

        let summary = scan_library_root(&root, &persistence).unwrap();
        assert_eq!(summary.discovered, 2);
        assert_eq!(summary.inserted, 2);
        assert_eq!(summary.metadata_failures, 0);
        assert!(summary.traversal_errors.is_empty());
        assert_eq!(persistence.tracks().unwrap().len(), 2);

        persistence.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rescan_marks_removed_tracks_missing_and_restores_them() {
        let root = temp_path("missing");
        fs::create_dir_all(&root).unwrap();
        let audio = root.join("tone.wav");
        fs::copy("tests/fixtures/audio/tone.wav", &audio).unwrap();
        let persistence = PersistenceWorker::start(root.join("library.sqlite")).unwrap();

        assert_eq!(scan_library_root(&root, &persistence).unwrap().inserted, 1);
        fs::remove_file(&audio).unwrap();
        assert_eq!(scan_library_root(&root, &persistence).unwrap().missing, 1);
        assert!(persistence.tracks().unwrap()[0].missing);

        fs::copy("tests/fixtures/audio/tone.wav", &audio).unwrap();
        assert_eq!(scan_library_root(&root, &persistence).unwrap().restored, 1);
        assert!(!persistence.tracks().unwrap()[0].missing);

        persistence.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_supported_file_is_indexed_with_metadata_failure() {
        let root = temp_path("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::copy("tests/fixtures/audio/corrupt.mp3", root.join("broken.mp3")).unwrap();
        let persistence = PersistenceWorker::start(root.join("library.sqlite")).unwrap();

        let summary = scan_library_root(&root, &persistence).unwrap();
        assert_eq!(summary.discovered, 1);
        assert_eq!(summary.inserted, 1);
        assert_eq!(summary.metadata_failures, 1);
        assert_eq!(
            persistence.tracks().unwrap()[0].title.as_deref(),
            Some("broken")
        );

        persistence.shutdown().unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
