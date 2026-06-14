use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension};

pub const SCHEMA_VERSION: i64 = 1;

const MIGRATION_1: &str = r#"
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE library_roots (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    display_name TEXT,
    recursive INTEGER NOT NULL DEFAULT 1 CHECK (recursive IN (0, 1)),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    last_scan_at_ms INTEGER,
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE tracks (
    id INTEGER PRIMARY KEY,
    library_root_id INTEGER REFERENCES library_roots(id) ON DELETE SET NULL,
    path TEXT NOT NULL UNIQUE,
    file_size INTEGER NOT NULL,
    modified_at_ms INTEGER NOT NULL,
    content_fingerprint TEXT,
    title TEXT,
    artist TEXT,
    album TEXT,
    genre TEXT,
    duration_frames INTEGER,
    sample_rate INTEGER,
    channels INTEGER,
    codec TEXT,
    missing INTEGER NOT NULL DEFAULT 0 CHECK (missing IN (0, 1)),
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX tracks_library_root_id ON tracks(library_root_id);
CREATE INDEX tracks_artist ON tracks(artist);
CREATE INDEX tracks_title ON tracks(title);
CREATE INDEX tracks_missing ON tracks(missing);
CREATE TABLE track_analysis (
    track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    analysis_version INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'complete', 'failed')),
    bpm REAL,
    bpm_confidence REAL,
    musical_key TEXT,
    key_confidence REAL,
    integrated_lufs REAL,
    true_peak_db REAL,
    beat_grid_path TEXT,
    waveform_path TEXT,
    error_message TEXT,
    analyzed_at_ms INTEGER
);
CREATE TABLE track_corrections (
    track_id INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
    bpm REAL,
    musical_key TEXT,
    beat_grid_offset_frames INTEGER,
    updated_at_ms INTEGER NOT NULL
);
CREATE TABLE hot_cues (
    id INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    slot INTEGER NOT NULL CHECK (slot >= 0),
    position_frames INTEGER NOT NULL CHECK (position_frames >= 0),
    label TEXT,
    color TEXT,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(track_id, slot)
);
CREATE TABLE saved_loops (
    id INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    slot INTEGER NOT NULL CHECK (slot >= 0),
    start_frame INTEGER NOT NULL CHECK (start_frame >= 0),
    end_frame INTEGER NOT NULL CHECK (end_frame > start_frame),
    label TEXT,
    updated_at_ms INTEGER NOT NULL,
    UNIQUE(track_id, slot)
);
CREATE TABLE queue_items (
    id INTEGER PRIMARY KEY,
    track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
    position INTEGER NOT NULL UNIQUE CHECK (position >= 0),
    added_at_ms INTEGER NOT NULL
);
"#;

#[derive(Debug)]
pub enum PersistenceError {
    Database(rusqlite::Error),
    UnsupportedSchema(i64),
    WorkerUnavailable,
    WorkerPanicked,
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "database error: {error}"),
            Self::UnsupportedSchema(version) => write!(
                formatter,
                "database schema version {version} is newer than supported version {SCHEMA_VERSION}"
            ),
            Self::WorkerUnavailable => write!(formatter, "persistence worker is unavailable"),
            Self::WorkerPanicked => write!(formatter, "persistence worker panicked"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Database(value)
    }
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTrack {
    pub library_root_id: Option<i64>,
    pub path: String,
    pub file_size: i64,
    pub modified_at_ms: i64,
    pub content_fingerprint: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub duration_frames: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub codec: Option<String>,
    pub missing: bool,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackChange {
    Inserted,
    Unchanged,
    Modified,
    Restored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackUpsert {
    pub id: i64,
    pub change: TrackChange,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisRecord {
    pub track_id: i64,
    pub analysis_version: i64,
    pub status: String,
    pub bpm: Option<f64>,
    pub bpm_confidence: Option<f64>,
    pub musical_key: Option<String>,
    pub key_confidence: Option<f64>,
    pub integrated_lufs: Option<f64>,
    pub true_peak_db: Option<f64>,
    pub beat_grid_path: Option<String>,
    pub waveform_path: Option<String>,
    pub error_message: Option<String>,
    pub analyzed_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryRootRecord {
    pub id: i64,
    pub path: String,
    pub display_name: Option<String>,
    pub last_scan_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackRecord {
    pub id: i64,
    pub library_root_id: Option<i64>,
    pub path: String,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_frames: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub codec: Option<String>,
    pub missing: bool,
}

pub struct Persistence {
    connection: Connection,
}

impl Persistence {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut connection: Connection) -> Result<Self> {
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(Duration::from_secs(2))?;
        migrate(&mut connection)?;
        Ok(Self { connection })
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))?)
    }

    pub fn set_setting(&mut self, key: &str, value: &str, updated_at_ms: i64) -> Result<()> {
        self.connection.execute(
            "INSERT INTO settings(key, value, updated_at_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at_ms = excluded.updated_at_ms",
            params![key, value, updated_at_ms],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .connection
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
                row.get(0)
            })
            .optional()?)
    }

    pub fn add_library_root(
        &mut self,
        path: &str,
        display_name: Option<&str>,
        now_ms: i64,
    ) -> Result<i64> {
        self.connection.execute(
            "INSERT INTO library_roots(path, display_name, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(path) DO UPDATE SET display_name=excluded.display_name,
             enabled=1, updated_at_ms=excluded.updated_at_ms",
            params![path, display_name, now_ms],
        )?;
        Ok(self.connection.query_row(
            "SELECT id FROM library_roots WHERE path=?1",
            [path],
            |row| row.get(0),
        )?)
    }

    pub fn library_roots(&self) -> Result<Vec<LibraryRootRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, display_name, last_scan_at_ms
             FROM library_roots WHERE enabled=1 ORDER BY display_name, path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(LibraryRootRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                display_name: row.get(2)?,
                last_scan_at_ms: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn upsert_track(&mut self, track: &NewTrack) -> Result<TrackUpsert> {
        let existing = self
            .connection
            .query_row(
                "SELECT id, file_size, modified_at_ms, content_fingerprint, missing
                 FROM tracks WHERE path = ?1",
                [&track.path],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, bool>(4)?,
                    ))
                },
            )
            .optional()?;

        if let Some((id, size, modified, fingerprint, was_missing)) = existing {
            let change = if was_missing && !track.missing {
                TrackChange::Restored
            } else if size == track.file_size
                && modified == track.modified_at_ms
                && fingerprint == track.content_fingerprint
                && was_missing == track.missing
            {
                TrackChange::Unchanged
            } else {
                TrackChange::Modified
            };
            self.connection.execute(
                "UPDATE tracks SET library_root_id=?2, file_size=?3, modified_at_ms=?4,
                 content_fingerprint=?5, title=?6, artist=?7, album=?8, genre=?9,
                 duration_frames=?10, sample_rate=?11, channels=?12, codec=?13,
                 missing=?14, updated_at_ms=?15 WHERE id=?1",
                params![
                    id,
                    track.library_root_id,
                    track.file_size,
                    track.modified_at_ms,
                    track.content_fingerprint,
                    track.title,
                    track.artist,
                    track.album,
                    track.genre,
                    track.duration_frames,
                    track.sample_rate,
                    track.channels,
                    track.codec,
                    track.missing,
                    track.updated_at_ms,
                ],
            )?;
            return Ok(TrackUpsert { id, change });
        }

        self.connection.execute(
            "INSERT INTO tracks(library_root_id, path, file_size, modified_at_ms,
             content_fingerprint, title, artist, album, genre, duration_frames,
             sample_rate, channels, codec, missing, created_at_ms, updated_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?15)",
            params![
                track.library_root_id,
                track.path,
                track.file_size,
                track.modified_at_ms,
                track.content_fingerprint,
                track.title,
                track.artist,
                track.album,
                track.genre,
                track.duration_frames,
                track.sample_rate,
                track.channels,
                track.codec,
                track.missing,
                track.updated_at_ms,
            ],
        )?;
        Ok(TrackUpsert {
            id: self.connection.last_insert_rowid(),
            change: TrackChange::Inserted,
        })
    }

    pub fn mark_track_missing(&mut self, id: i64, updated_at_ms: i64) -> Result<()> {
        self.connection.execute(
            "UPDATE tracks SET missing = 1, updated_at_ms = ?2 WHERE id = ?1",
            params![id, updated_at_ms],
        )?;
        Ok(())
    }

    pub fn reconcile_library_root(
        &mut self,
        library_root_id: i64,
        seen_paths: &[String],
        scanned_at_ms: i64,
    ) -> Result<usize> {
        let transaction = self.connection.transaction()?;
        transaction.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS current_scan_paths(
                 path TEXT PRIMARY KEY
             );
             DELETE FROM current_scan_paths;",
        )?;
        {
            let mut insert = transaction
                .prepare("INSERT OR IGNORE INTO current_scan_paths(path) VALUES (?1)")?;
            for path in seen_paths {
                insert.execute([path])?;
            }
        }
        let missing = transaction.execute(
            "UPDATE tracks SET missing=1, updated_at_ms=?2
             WHERE library_root_id=?1 AND missing=0
             AND NOT EXISTS (SELECT 1 FROM current_scan_paths WHERE path=tracks.path)",
            params![library_root_id, scanned_at_ms],
        )?;
        transaction.execute(
            "UPDATE library_roots SET last_scan_at_ms=?2, updated_at_ms=?2 WHERE id=?1",
            params![library_root_id, scanned_at_ms],
        )?;
        transaction.commit()?;
        Ok(missing)
    }

    pub fn tracks(&self) -> Result<Vec<TrackRecord>> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_root_id, path, title, artist, duration_frames,
             sample_rate, channels, codec, missing
             FROM tracks ORDER BY missing, artist, title, path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(TrackRecord {
                id: row.get(0)?,
                library_root_id: row.get(1)?,
                path: row.get(2)?,
                title: row.get(3)?,
                artist: row.get(4)?,
                duration_frames: row.get(5)?,
                sample_rate: row.get(6)?,
                channels: row.get(7)?,
                codec: row.get(8)?,
                missing: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn track(&self, id: i64) -> Result<Option<TrackRecord>> {
        Ok(self
            .connection
            .query_row(
                "SELECT id, library_root_id, path, title, artist, duration_frames,
                 sample_rate, channels, codec, missing FROM tracks WHERE id=?1",
                [id],
                |row| {
                    Ok(TrackRecord {
                        id: row.get(0)?,
                        library_root_id: row.get(1)?,
                        path: row.get(2)?,
                        title: row.get(3)?,
                        artist: row.get(4)?,
                        duration_frames: row.get(5)?,
                        sample_rate: row.get(6)?,
                        channels: row.get(7)?,
                        codec: row.get(8)?,
                        missing: row.get(9)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_analysis(&mut self, analysis: &AnalysisRecord) -> Result<()> {
        self.connection.execute(
            "INSERT INTO track_analysis(track_id, analysis_version, status, bpm,
             bpm_confidence, musical_key, key_confidence, integrated_lufs, true_peak_db,
             beat_grid_path, waveform_path, error_message, analyzed_at_ms)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
             ON CONFLICT(track_id) DO UPDATE SET analysis_version=excluded.analysis_version,
             status=excluded.status, bpm=excluded.bpm, bpm_confidence=excluded.bpm_confidence,
             musical_key=excluded.musical_key, key_confidence=excluded.key_confidence,
             integrated_lufs=excluded.integrated_lufs, true_peak_db=excluded.true_peak_db,
             beat_grid_path=excluded.beat_grid_path, waveform_path=excluded.waveform_path,
             error_message=excluded.error_message, analyzed_at_ms=excluded.analyzed_at_ms",
            params![
                analysis.track_id,
                analysis.analysis_version,
                analysis.status,
                analysis.bpm,
                analysis.bpm_confidence,
                analysis.musical_key,
                analysis.key_confidence,
                analysis.integrated_lufs,
                analysis.true_peak_db,
                analysis.beat_grid_path,
                analysis.waveform_path,
                analysis.error_message,
                analysis.analyzed_at_ms,
            ],
        )?;
        Ok(())
    }

    pub fn replace_queue(&mut self, track_ids: &[i64], added_at_ms: i64) -> Result<()> {
        let transaction = self.connection.transaction()?;
        transaction.execute("DELETE FROM queue_items", [])?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO queue_items(track_id, position, added_at_ms) VALUES (?1, ?2, ?3)",
            )?;
            for (position, track_id) in track_ids.iter().enumerate() {
                statement.execute(params![track_id, position as i64, added_at_ms])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn queue(&self) -> Result<Vec<i64>> {
        let mut statement = self
            .connection
            .prepare("SELECT track_id FROM queue_items ORDER BY position")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn migrate(connection: &mut Connection) -> Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(PersistenceError::UnsupportedSchema(version));
    }
    if version == 0 {
        apply_migration(connection, SCHEMA_VERSION, MIGRATION_1)?;
    }
    Ok(())
}

fn apply_migration(connection: &mut Connection, version: i64, sql: &str) -> Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(sql)?;
    transaction.pragma_update(None, "user_version", version)?;
    transaction.commit()?;
    Ok(())
}

type Response<T> = Sender<Result<T>>;

enum Command {
    SetSetting {
        key: String,
        value: String,
        updated_at_ms: i64,
        response: Response<()>,
    },
    GetSetting {
        key: String,
        response: Response<Option<String>>,
    },
    AddLibraryRoot {
        path: String,
        display_name: Option<String>,
        now_ms: i64,
        response: Response<i64>,
    },
    GetLibraryRoots {
        response: Response<Vec<LibraryRootRecord>>,
    },
    UpsertTrack {
        track: Box<NewTrack>,
        response: Response<TrackUpsert>,
    },
    MarkTrackMissing {
        id: i64,
        updated_at_ms: i64,
        response: Response<()>,
    },
    ReconcileLibraryRoot {
        library_root_id: i64,
        seen_paths: Vec<String>,
        scanned_at_ms: i64,
        response: Response<usize>,
    },
    GetTracks {
        response: Response<Vec<TrackRecord>>,
    },
    GetTrack {
        id: i64,
        response: Response<Option<TrackRecord>>,
    },
    SaveAnalysis {
        analysis: Box<AnalysisRecord>,
        response: Response<()>,
    },
    ReplaceQueue {
        track_ids: Vec<i64>,
        added_at_ms: i64,
        response: Response<()>,
    },
    GetQueue {
        response: Response<Vec<i64>>,
    },
    Shutdown,
}

pub struct PersistenceWorker {
    sender: Sender<Command>,
    join: Option<JoinHandle<()>>,
}

impl PersistenceWorker {
    pub fn start(path: PathBuf) -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let (startup_sender, startup_receiver) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("djapp-persistence".to_string())
            .spawn(move || match Persistence::open(path) {
                Ok(database) => {
                    let _ = startup_sender.send(Ok(()));
                    run_worker(database, receiver);
                }
                Err(error) => {
                    let _ = startup_sender.send(Err(error));
                }
            })
            .map_err(|_| PersistenceError::WorkerUnavailable)?;
        startup_receiver
            .recv()
            .map_err(|_| PersistenceError::WorkerUnavailable)??;
        Ok(Self {
            sender,
            join: Some(join),
        })
    }

    pub fn set_setting(&self, key: String, value: String, updated_at_ms: i64) -> Result<()> {
        self.request(|response| Command::SetSetting {
            key,
            value,
            updated_at_ms,
            response,
        })
    }

    pub fn setting(&self, key: String) -> Result<Option<String>> {
        self.request(|response| Command::GetSetting { key, response })
    }

    pub fn add_library_root(
        &self,
        path: String,
        display_name: Option<String>,
        now_ms: i64,
    ) -> Result<i64> {
        self.request(|response| Command::AddLibraryRoot {
            path,
            display_name,
            now_ms,
            response,
        })
    }

    pub fn library_roots(&self) -> Result<Vec<LibraryRootRecord>> {
        self.request(|response| Command::GetLibraryRoots { response })
    }

    pub fn upsert_track(&self, track: NewTrack) -> Result<TrackUpsert> {
        self.request(|response| Command::UpsertTrack {
            track: Box::new(track),
            response,
        })
    }

    pub fn mark_track_missing(&self, id: i64, updated_at_ms: i64) -> Result<()> {
        self.request(|response| Command::MarkTrackMissing {
            id,
            updated_at_ms,
            response,
        })
    }

    pub fn reconcile_library_root(
        &self,
        library_root_id: i64,
        seen_paths: Vec<String>,
        scanned_at_ms: i64,
    ) -> Result<usize> {
        self.request(|response| Command::ReconcileLibraryRoot {
            library_root_id,
            seen_paths,
            scanned_at_ms,
            response,
        })
    }

    pub fn tracks(&self) -> Result<Vec<TrackRecord>> {
        self.request(|response| Command::GetTracks { response })
    }

    pub fn track(&self, id: i64) -> Result<Option<TrackRecord>> {
        self.request(|response| Command::GetTrack { id, response })
    }

    pub fn save_analysis(&self, analysis: AnalysisRecord) -> Result<()> {
        self.request(|response| Command::SaveAnalysis {
            analysis: Box::new(analysis),
            response,
        })
    }

    pub fn replace_queue(&self, track_ids: Vec<i64>, added_at_ms: i64) -> Result<()> {
        self.request(|response| Command::ReplaceQueue {
            track_ids,
            added_at_ms,
            response,
        })
    }

    pub fn queue(&self) -> Result<Vec<i64>> {
        self.request(|response| Command::GetQueue { response })
    }

    fn request<T>(&self, build: impl FnOnce(Response<T>) -> Command) -> Result<T> {
        let (response, receiver) = mpsc::channel();
        self.sender
            .send(build(response))
            .map_err(|_| PersistenceError::WorkerUnavailable)?;
        receiver
            .recv()
            .map_err(|_| PersistenceError::WorkerUnavailable)?
    }

    pub fn shutdown(mut self) -> Result<()> {
        let _ = self.sender.send(Command::Shutdown);
        self.join
            .take()
            .expect("persistence worker join handle must exist")
            .join()
            .map_err(|_| PersistenceError::WorkerPanicked)
    }
}

impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        let _ = self.sender.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn run_worker(mut database: Persistence, receiver: Receiver<Command>) {
    while let Ok(command) = receiver.recv() {
        match command {
            Command::SetSetting {
                key,
                value,
                updated_at_ms,
                response,
            } => {
                let _ = response.send(database.set_setting(&key, &value, updated_at_ms));
            }
            Command::GetSetting { key, response } => {
                let _ = response.send(database.setting(&key));
            }
            Command::AddLibraryRoot {
                path,
                display_name,
                now_ms,
                response,
            } => {
                let _ = response.send(database.add_library_root(
                    &path,
                    display_name.as_deref(),
                    now_ms,
                ));
            }
            Command::GetLibraryRoots { response } => {
                let _ = response.send(database.library_roots());
            }
            Command::UpsertTrack { track, response } => {
                let _ = response.send(database.upsert_track(track.as_ref()));
            }
            Command::MarkTrackMissing {
                id,
                updated_at_ms,
                response,
            } => {
                let _ = response.send(database.mark_track_missing(id, updated_at_ms));
            }
            Command::ReconcileLibraryRoot {
                library_root_id,
                seen_paths,
                scanned_at_ms,
                response,
            } => {
                let _ = response.send(database.reconcile_library_root(
                    library_root_id,
                    &seen_paths,
                    scanned_at_ms,
                ));
            }
            Command::GetTracks { response } => {
                let _ = response.send(database.tracks());
            }
            Command::GetTrack { id, response } => {
                let _ = response.send(database.track(id));
            }
            Command::SaveAnalysis { analysis, response } => {
                let _ = response.send(database.save_analysis(analysis.as_ref()));
            }
            Command::ReplaceQueue {
                track_ids,
                added_at_ms,
                response,
            } => {
                let _ = response.send(database.replace_queue(&track_ids, added_at_ms));
            }
            Command::GetQueue { response } => {
                let _ = response.send(database.queue());
            }
            Command::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    fn track(path: &str, root: Option<i64>) -> NewTrack {
        NewTrack {
            library_root_id: root,
            path: path.to_string(),
            file_size: 100,
            modified_at_ms: 10,
            content_fingerprint: Some("fingerprint-a".to_string()),
            title: Some("Title".to_string()),
            artist: Some("Artist".to_string()),
            album: None,
            genre: None,
            duration_frames: Some(44_100),
            sample_rate: Some(44_100),
            channels: Some(2),
            codec: Some("wav".to_string()),
            missing: false,
            updated_at_ms: 1000,
        }
    }

    fn temporary_database(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "djapp-{name}-{}-{nonce}.sqlite",
            std::process::id()
        ))
    }

    #[test]
    fn creates_schema_and_rejects_newer_versions() {
        let database = Persistence::open_in_memory().unwrap();
        assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);

        let connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        assert!(matches!(
            Persistence::from_connection(connection),
            Err(PersistenceError::UnsupportedSchema(2))
        ));
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_version() {
        let mut connection = Connection::open_in_memory().unwrap();
        assert!(apply_migration(
            &mut connection,
            1,
            "CREATE TABLE survives(value INTEGER); THIS IS INVALID SQL;"
        )
        .is_err());
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let table: Option<String> = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='survives'",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        assert_eq!(version, 0);
        assert_eq!(table, None);
    }

    #[test]
    fn settings_and_queue_survive_reopen_through_worker() {
        let path = temporary_database("reopen");
        let worker = PersistenceWorker::start(path.clone()).unwrap();
        worker
            .set_setting("theme".to_string(), "dark".to_string(), 1)
            .unwrap();
        let first = worker.upsert_track(track("/music/a.wav", None)).unwrap();
        let second = worker.upsert_track(track("/music/b.wav", None)).unwrap();
        worker.replace_queue(vec![second.id, first.id], 2).unwrap();
        worker.shutdown().unwrap();

        let worker = PersistenceWorker::start(path.clone()).unwrap();
        assert_eq!(
            worker.setting("theme".to_string()).unwrap().as_deref(),
            Some("dark")
        );
        assert_eq!(worker.queue().unwrap(), vec![second.id, first.id]);
        worker.shutdown().unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn track_upsert_reports_file_state_changes() {
        let mut database = Persistence::open_in_memory().unwrap();
        let root = database
            .add_library_root("/music", Some("Music"), 1)
            .unwrap();
        let initial = track("/music/a.wav", Some(root));
        let inserted = database.upsert_track(&initial).unwrap();
        assert_eq!(inserted.change, TrackChange::Inserted);
        assert_eq!(
            database.upsert_track(&initial).unwrap().change,
            TrackChange::Unchanged
        );

        let mut modified = initial.clone();
        modified.file_size = 101;
        assert_eq!(
            database.upsert_track(&modified).unwrap().change,
            TrackChange::Modified
        );
        database.mark_track_missing(inserted.id, 2).unwrap();
        assert_eq!(
            database.upsert_track(&modified).unwrap().change,
            TrackChange::Restored
        );
    }

    #[test]
    fn constraints_cascades_and_analysis_preserve_user_data() {
        let mut database = Persistence::open_in_memory().unwrap();
        let track_id = database
            .upsert_track(&track("/music/a.wav", None))
            .unwrap()
            .id;
        database
            .connection
            .execute(
                "INSERT INTO track_corrections(track_id, bpm, updated_at_ms) VALUES (?1, 121.0, 1)",
                [track_id],
            )
            .unwrap();
        database
            .connection
            .execute(
                "INSERT INTO hot_cues(track_id, slot, position_frames, updated_at_ms) VALUES (?1, 0, 100, 1)",
                [track_id],
            )
            .unwrap();
        assert!(database
            .connection
            .execute(
                "INSERT INTO saved_loops(track_id, slot, start_frame, end_frame, updated_at_ms) VALUES (?1, 0, 100, 50, 1)",
                [track_id],
            )
            .is_err());

        let mut analysis = AnalysisRecord {
            track_id,
            analysis_version: 1,
            status: "complete".to_string(),
            bpm: Some(120.0),
            bpm_confidence: Some(0.8),
            musical_key: Some("8A".to_string()),
            key_confidence: Some(0.7),
            integrated_lufs: Some(-14.0),
            true_peak_db: Some(-1.0),
            beat_grid_path: Some("grid-v1".to_string()),
            waveform_path: Some("wave-v1".to_string()),
            error_message: None,
            analyzed_at_ms: Some(1),
        };
        database.save_analysis(&analysis).unwrap();
        analysis.analysis_version = 2;
        analysis.bpm = Some(122.0);
        database.save_analysis(&analysis).unwrap();
        let correction: f64 = database
            .connection
            .query_row(
                "SELECT bpm FROM track_corrections WHERE track_id=?1",
                [track_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(correction, 121.0);

        database
            .connection
            .execute("DELETE FROM tracks WHERE id=?1", [track_id])
            .unwrap();
        for table in ["track_analysis", "track_corrections", "hot_cues"] {
            let count: i64 = database
                .connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0);
        }
    }
}
