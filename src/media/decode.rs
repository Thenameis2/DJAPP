use std::{
    error::Error,
    fs::File,
    path::{Path, PathBuf},
};

use symphonia::core::{
    audio::sample::Sample,
    codecs::audio::{AudioDecoder as SymphoniaAudioDecoder, AudioDecoderOptions},
    errors::Error as SymphoniaError,
    formats::{probe::Hint, FormatOptions, FormatReader, SeekMode, SeekTo, TrackType},
    io::MediaSourceStream,
    meta::{MetadataOptions, StandardTag},
    units::Time,
};

pub const MAX_DECODED_FRAMES_PER_CHUNK: usize = 16_384;

#[derive(Clone, Debug, PartialEq)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_seconds: Option<f64>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PcmChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: usize,
}

impl PcmChunk {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels
    }
}

pub struct MediaDecoder {
    info: MediaInfo,
    track_id: u32,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaAudioDecoder>,
}

impl MediaDecoder {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let stream = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
            hint.with_extension(extension);
        }

        let mut format = symphonia::default::get_probe().probe(
            &hint,
            stream,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let track = format
            .default_track(TrackType::Audio)
            .cloned()
            .ok_or("media has no default audio track")?;
        let track_id = track.id;
        let codec_params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .ok_or("audio track has no codec parameters")?;
        let sample_rate = codec_params
            .sample_rate
            .ok_or("audio track has no sample rate")?;
        let channels = codec_params
            .channels
            .as_ref()
            .ok_or("audio track has no channel layout")?
            .count();
        let duration_seconds = track
            .time_base
            .zip(track.duration)
            .map(|(time_base, duration)| f64::from(time_base) * duration.get() as f64);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(codec_params, &AudioDecoderOptions::default())?;
        let codec = decoder.codec_info().short_name.to_string();
        let (title, artist) = read_standard_metadata(&mut *format);

        Ok(Self {
            info: MediaInfo {
                path: path.to_path_buf(),
                codec,
                sample_rate,
                channels,
                duration_seconds,
                title,
                artist,
            },
            track_id,
            format,
            decoder,
        })
    }

    pub fn info(&self) -> &MediaInfo {
        &self.info
    }

    pub fn seek(&mut self, seconds: f64) -> Result<f64, Box<dyn Error>> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err("seek position must be a finite non-negative number".into());
        }

        let whole_seconds = seconds.floor() as i64;
        let nanos = ((seconds - whole_seconds as f64) * 1_000_000_000.0).round() as u32;
        let time = Time::try_new(whole_seconds, nanos.min(999_999_999))
            .ok_or("seek position is out of range")?;
        let seeked = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        )?;
        self.decoder.reset();

        let track = self
            .format
            .tracks()
            .iter()
            .find(|track| track.id == seeked.track_id)
            .ok_or("seek returned an unknown track")?;
        let actual = track
            .time_base
            .and_then(|time_base| time_base.calc_time(seeked.actual_ts))
            .ok_or("seek result has no usable time base")?;
        Ok(actual.as_secs_f64())
    }

    pub fn next_chunk(&mut self) -> Result<Option<PcmChunk>, Box<dyn Error>> {
        self.next_chunk_into(Vec::new())
    }

    pub fn next_chunk_into(
        &mut self,
        mut samples: Vec<f32>,
    ) -> Result<Option<PcmChunk>, Box<dyn Error>> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(None),
                Err(SymphoniaError::ResetRequired) => {
                    return Err("media track changed during decoding".into())
                }
                Err(error) => return Err(error.into()),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(audio) => {
                    let channels = audio.spec().channels().count();
                    let sample_rate = audio.spec().rate();
                    samples.resize(audio.samples_interleaved(), f32::MID);
                    audio.copy_to_slice_interleaved(&mut samples);

                    if samples.is_empty() {
                        continue;
                    }
                    if samples.len() / channels > MAX_DECODED_FRAMES_PER_CHUNK {
                        return Err("decoder produced an unexpectedly large PCM chunk".into());
                    }

                    return Ok(Some(PcmChunk {
                        samples,
                        sample_rate,
                        channels,
                    }));
                }
                Err(SymphoniaError::DecodeError(_)) | Err(SymphoniaError::IoError(_)) => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    #[cfg(test)]
    fn decode_all(mut self) -> Result<Vec<f32>, Box<dyn Error>> {
        let mut samples = Vec::new();
        while let Some(chunk) = self.next_chunk()? {
            samples.extend_from_slice(&chunk.samples);
        }
        Ok(samples)
    }
}

fn read_standard_metadata(format: &mut dyn FormatReader) -> (Option<String>, Option<String>) {
    let mut title = None;
    let mut artist = None;
    let mut metadata = format.metadata();
    let Some(revision) = metadata.skip_to_latest() else {
        return (title, artist);
    };

    for tag in &revision.media.tags {
        match tag.std.as_ref() {
            Some(StandardTag::TrackTitle(value)) => title = Some(value.to_string()),
            Some(StandardTag::Artist(value)) => artist = Some(value.to_string()),
            _ => {}
        }
    }

    (title, artist)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_DIR: &str = "tests/fixtures/audio";
    const SUPPORTED_FIXTURES: &[&str] =
        &["tone.wav", "tone.aiff", "tone.flac", "tone.mp3", "tone.m4a"];

    #[test]
    fn decodes_supported_formats_to_interleaved_f32() {
        for name in SUPPORTED_FIXTURES {
            let path = Path::new(FIXTURE_DIR).join(name);
            let decoder = MediaDecoder::open(&path)
                .unwrap_or_else(|error| panic!("failed to open {}: {error}", path.display()));
            let info = decoder.info().clone();
            let samples = decoder
                .decode_all()
                .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));

            assert_eq!(info.sample_rate, 44_100, "{}", path.display());
            assert_eq!(info.channels, 2, "{}", path.display());
            assert!(!info.codec.is_empty(), "{}", path.display());
            assert!(samples.len() > 44_100 * 2, "{}", path.display());
            assert_eq!(samples.len() % info.channels, 0, "{}", path.display());
            assert!(
                samples.iter().all(|sample| sample.is_finite()),
                "{}",
                path.display()
            );
            assert!(
                samples.iter().any(|sample| sample.abs() > 0.01),
                "{}",
                path.display()
            );
        }
    }

    #[test]
    fn seeks_to_the_middle_of_supported_formats() {
        for name in SUPPORTED_FIXTURES {
            let path = Path::new(FIXTURE_DIR).join(name);
            let mut decoder = MediaDecoder::open(&path).unwrap();
            let actual = decoder
                .seek(1.5)
                .unwrap_or_else(|error| panic!("failed to seek {}: {error}", path.display()));
            let chunk = decoder
                .next_chunk()
                .unwrap_or_else(|error| panic!("failed after seek {}: {error}", path.display()))
                .unwrap_or_else(|| panic!("no samples after seek: {}", path.display()));

            assert!(actual <= 1.5, "{} seeked to {actual}", path.display());
            assert!(actual >= 0.0, "{} seeked to {actual}", path.display());
            assert_eq!(chunk.sample_rate, 44_100, "{}", path.display());
            assert_eq!(chunk.channels, 2, "{}", path.display());
            assert!(chunk.frames() > 0, "{}", path.display());
        }
    }

    #[test]
    fn rejects_corrupt_media_without_panicking() {
        let path = Path::new(FIXTURE_DIR).join("corrupt.mp3");
        let result = MediaDecoder::open(path);
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_seek_positions() {
        let path = Path::new(FIXTURE_DIR).join("tone.wav");
        let mut decoder = MediaDecoder::open(path).unwrap();
        assert!(decoder.seek(-1.0).is_err());
        assert!(decoder.seek(f64::NAN).is_err());
    }

    #[test]
    fn reads_standard_title_and_artist_metadata() {
        for name in ["tone.mp3", "tone.m4a"] {
            let path = Path::new(FIXTURE_DIR).join(name);
            let decoder = MediaDecoder::open(&path).unwrap();
            assert_eq!(decoder.info().title.as_deref(), Some("Decoder Fixture"));
            assert_eq!(decoder.info().artist.as_deref(), Some("DJ App Tests"));
        }
    }
}
