use std::{error::Error, fmt};

use super::ANALYSIS_VERSION;

pub const WAVEFORM_FORMAT_VERSION: u16 = 1;
pub const BEAT_GRID_FORMAT_VERSION: u16 = 1;

const WAVEFORM_MAGIC: [u8; 8] = *b"DJWAVE01";
const BEAT_GRID_MAGIC: [u8; 8] = *b"DJBEAT01";
const COMMON_HEADER_BYTES: usize = 68;
const BEAT_RECORD_BYTES: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct WaveformLevel {
    pub bucket_frames: u32,
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaveformCache {
    pub identity_digest: [u8; 32],
    pub source_sample_rate: u32,
    pub source_channels: u16,
    pub source_frames: u64,
    pub levels: Vec<WaveformLevel>,
}

impl WaveformCache {
    pub fn encode(&self) -> Result<Vec<u8>, CacheError> {
        validate_common(
            self.source_sample_rate,
            self.source_channels,
            self.source_frames,
        )?;
        let channels = usize::from(self.source_channels);
        let values_per_bucket = channels.checked_mul(3).ok_or(CacheError::TooLarge)?;
        let mut output = Vec::new();
        write_common_header(
            &mut output,
            WAVEFORM_MAGIC,
            WAVEFORM_FORMAT_VERSION,
            self.identity_digest,
            self.source_sample_rate,
            self.source_channels,
            self.source_frames,
        );
        write_u32(&mut output, usize_to_u32(self.levels.len())?);
        for level in &self.levels {
            if level.bucket_frames == 0
                || level.values.iter().any(|value| !value.is_finite())
                || level.values.len() % values_per_bucket != 0
            {
                return Err(CacheError::InvalidData);
            }
            write_u32(&mut output, level.bucket_frames);
            write_u32(
                &mut output,
                usize_to_u32(level.values.len() / values_per_bucket)?,
            );
            for value in &level.values {
                write_f32(&mut output, *value);
            }
        }
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CacheError> {
        let mut reader = Reader::new(bytes);
        let header = reader.read_common_header(WAVEFORM_MAGIC, WAVEFORM_FORMAT_VERSION)?;
        let level_count = u32_to_usize(reader.read_u32()?)?;
        if level_count > reader.remaining() / 8 {
            return Err(CacheError::InvalidLength);
        }
        let channels = usize::from(header.source_channels);
        let values_per_bucket = channels.checked_mul(3).ok_or(CacheError::TooLarge)?;
        let mut levels = Vec::with_capacity(level_count);
        for _ in 0..level_count {
            let bucket_frames = reader.read_u32()?;
            let bucket_count = u32_to_usize(reader.read_u32()?)?;
            let value_count = bucket_count
                .checked_mul(values_per_bucket)
                .ok_or(CacheError::TooLarge)?;
            if bucket_frames == 0 {
                return Err(CacheError::InvalidData);
            }
            let mut values = Vec::with_capacity(value_count);
            for _ in 0..value_count {
                let value = reader.read_f32()?;
                if !value.is_finite() {
                    return Err(CacheError::InvalidData);
                }
                values.push(value);
            }
            levels.push(WaveformLevel {
                bucket_frames,
                values,
            });
        }
        reader.finish()?;
        Ok(Self {
            identity_digest: header.identity_digest,
            source_sample_rate: header.source_sample_rate,
            source_channels: header.source_channels,
            source_frames: header.source_frames,
            levels,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BeatRecord {
    pub source_frame: u64,
    pub strength: f32,
    pub downbeat: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BeatGridCache {
    pub identity_digest: [u8; 32],
    pub source_sample_rate: u32,
    pub source_channels: u16,
    pub source_frames: u64,
    pub bpm: f64,
    pub confidence: f32,
    pub beats: Vec<BeatRecord>,
}

impl BeatGridCache {
    pub fn encode(&self) -> Result<Vec<u8>, CacheError> {
        validate_common(
            self.source_sample_rate,
            self.source_channels,
            self.source_frames,
        )?;
        validate_grid(self.bpm, self.confidence, self.source_frames, &self.beats)?;
        let mut output = Vec::new();
        write_common_header(
            &mut output,
            BEAT_GRID_MAGIC,
            BEAT_GRID_FORMAT_VERSION,
            self.identity_digest,
            self.source_sample_rate,
            self.source_channels,
            self.source_frames,
        );
        write_f64(&mut output, self.bpm);
        write_f32(&mut output, self.confidence);
        write_u32(&mut output, usize_to_u32(self.beats.len())?);
        for beat in &self.beats {
            write_u64(&mut output, beat.source_frame);
            write_f32(&mut output, beat.strength);
            output.push(u8::from(beat.downbeat));
            output.extend_from_slice(&[0; 3]);
        }
        Ok(output)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CacheError> {
        let mut reader = Reader::new(bytes);
        let header = reader.read_common_header(BEAT_GRID_MAGIC, BEAT_GRID_FORMAT_VERSION)?;
        let bpm = reader.read_f64()?;
        let confidence = reader.read_f32()?;
        let beat_count = u32_to_usize(reader.read_u32()?)?;
        let expected = beat_count
            .checked_mul(BEAT_RECORD_BYTES)
            .ok_or(CacheError::TooLarge)?;
        if reader.remaining() != expected {
            return Err(CacheError::InvalidLength);
        }
        let mut beats = Vec::with_capacity(beat_count);
        for _ in 0..beat_count {
            let source_frame = reader.read_u64()?;
            let strength = reader.read_f32()?;
            let downbeat = match reader.read_u8()? {
                0 => false,
                1 => true,
                _ => return Err(CacheError::InvalidData),
            };
            reader.skip(3)?;
            beats.push(BeatRecord {
                source_frame,
                strength,
                downbeat,
            });
        }
        validate_grid(bpm, confidence, header.source_frames, &beats)?;
        reader.finish()?;
        Ok(Self {
            identity_digest: header.identity_digest,
            source_sample_rate: header.source_sample_rate,
            source_channels: header.source_channels,
            source_frames: header.source_frames,
            bpm,
            confidence,
            beats,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheError {
    InvalidMagic,
    UnsupportedVersion,
    InvalidLength,
    InvalidData,
    TooLarge,
}

impl fmt::Display for CacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMagic => formatter.write_str("invalid analysis cache magic"),
            Self::UnsupportedVersion => formatter.write_str("unsupported analysis cache version"),
            Self::InvalidLength => formatter.write_str("invalid analysis cache length"),
            Self::InvalidData => formatter.write_str("invalid analysis cache data"),
            Self::TooLarge => formatter.write_str("analysis cache is too large"),
        }
    }
}

impl Error for CacheError {}

struct CommonHeader {
    identity_digest: [u8; 32],
    source_sample_rate: u32,
    source_channels: u16,
    source_frames: u64,
}

fn validate_common(sample_rate: u32, channels: u16, frames: u64) -> Result<(), CacheError> {
    if sample_rate == 0 || channels == 0 || frames == 0 {
        return Err(CacheError::InvalidData);
    }
    Ok(())
}

fn validate_grid(
    bpm: f64,
    confidence: f32,
    source_frames: u64,
    beats: &[BeatRecord],
) -> Result<(), CacheError> {
    if !bpm.is_finite()
        || bpm <= 0.0
        || !confidence.is_finite()
        || !(0.0..=1.0).contains(&confidence)
    {
        return Err(CacheError::InvalidData);
    }
    let mut previous = None;
    for beat in beats {
        if beat.source_frame >= source_frames
            || !beat.strength.is_finite()
            || beat.strength < 0.0
            || previous.is_some_and(|frame| beat.source_frame <= frame)
        {
            return Err(CacheError::InvalidData);
        }
        previous = Some(beat.source_frame);
    }
    Ok(())
}

fn write_common_header(
    output: &mut Vec<u8>,
    magic: [u8; 8],
    format_version: u16,
    identity_digest: [u8; 32],
    sample_rate: u32,
    channels: u16,
    frames: u64,
) {
    output.extend_from_slice(&magic);
    write_u16(output, format_version);
    write_u16(output, 0);
    write_u32(output, ANALYSIS_VERSION);
    output.extend_from_slice(&identity_digest);
    write_u32(output, sample_rate);
    write_u16(output, channels);
    write_u16(output, 0);
    write_u64(output, frames);
    write_u32(output, 0);
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_common_header(
        &mut self,
        expected_magic: [u8; 8],
        expected_format: u16,
    ) -> Result<CommonHeader, CacheError> {
        if self.bytes.len() < COMMON_HEADER_BYTES {
            return Err(CacheError::InvalidLength);
        }
        if self.take(8)? != expected_magic {
            return Err(CacheError::InvalidMagic);
        }
        if self.read_u16()? != expected_format || self.read_u16()? != 0 {
            return Err(CacheError::UnsupportedVersion);
        }
        if self.read_u32()? != ANALYSIS_VERSION {
            return Err(CacheError::UnsupportedVersion);
        }
        let mut identity_digest = [0; 32];
        identity_digest.copy_from_slice(self.take(32)?);
        let source_sample_rate = self.read_u32()?;
        let source_channels = self.read_u16()?;
        if self.read_u16()? != 0 {
            return Err(CacheError::UnsupportedVersion);
        }
        let source_frames = self.read_u64()?;
        if self.read_u32()? != 0 {
            return Err(CacheError::UnsupportedVersion);
        }
        validate_common(source_sample_rate, source_channels, source_frames)?;
        Ok(CommonHeader {
            identity_digest,
            source_sample_rate,
            source_channels,
            source_frames,
        })
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], CacheError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CacheError::TooLarge)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(CacheError::InvalidLength)?;
        self.offset = end;
        Ok(value)
    }

    fn read_u8(&mut self) -> Result<u8, CacheError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, CacheError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn read_u32(&mut self) -> Result<u32, CacheError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn read_u64(&mut self) -> Result<u64, CacheError> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn read_f32(&mut self) -> Result<f32, CacheError> {
        Ok(f32::from_bits(self.read_u32()?))
    }

    fn read_f64(&mut self) -> Result<f64, CacheError> {
        Ok(f64::from_bits(self.read_u64()?))
    }

    fn skip(&mut self, length: usize) -> Result<(), CacheError> {
        self.take(length).map(|_| ())
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn finish(self) -> Result<(), CacheError> {
        (self.offset == self.bytes.len())
            .then_some(())
            .ok_or(CacheError::InvalidLength)
    }
}

fn write_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(output: &mut Vec<u8>, value: f32) {
    write_u32(output, value.to_bits());
}

fn write_f64(output: &mut Vec<u8>, value: f64) {
    write_u64(output, value.to_bits());
}

fn usize_to_u32(value: usize) -> Result<u32, CacheError> {
    u32::try_from(value).map_err(|_| CacheError::TooLarge)
}

fn u32_to_usize(value: u32) -> Result<usize, CacheError> {
    usize::try_from(value).map_err(|_| CacheError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_cache_round_trips_and_rejects_truncation() {
        let cache = WaveformCache {
            identity_digest: [7; 32],
            source_sample_rate: 48_000,
            source_channels: 2,
            source_frames: 2_048,
            levels: vec![WaveformLevel {
                bucket_frames: 256,
                values: vec![-0.8, 0.9, 0.3, -0.7, 0.75, 0.25],
            }],
        };
        let encoded = cache.encode().unwrap();
        assert_eq!(WaveformCache::decode(&encoded).unwrap(), cache);
        assert_eq!(
            WaveformCache::decode(&encoded[..encoded.len() - 1]),
            Err(CacheError::InvalidLength)
        );
    }

    #[test]
    fn beat_grid_round_trips_and_rejects_non_monotonic_beats() {
        let mut cache = BeatGridCache {
            identity_digest: [9; 32],
            source_sample_rate: 44_100,
            source_channels: 2,
            source_frames: 100_000,
            bpm: 120.0,
            confidence: 0.91,
            beats: vec![
                BeatRecord {
                    source_frame: 0,
                    strength: 1.0,
                    downbeat: true,
                },
                BeatRecord {
                    source_frame: 22_050,
                    strength: 0.8,
                    downbeat: false,
                },
            ],
        };
        let encoded = cache.encode().unwrap();
        assert_eq!(BeatGridCache::decode(&encoded).unwrap(), cache);
        cache.beats[1].source_frame = 0;
        assert_eq!(cache.encode(), Err(CacheError::InvalidData));
    }

    #[test]
    fn cache_versions_are_enforced() {
        let cache = BeatGridCache {
            identity_digest: [0; 32],
            source_sample_rate: 48_000,
            source_channels: 1,
            source_frames: 48_000,
            bpm: 100.0,
            confidence: 0.5,
            beats: Vec::new(),
        };
        let mut encoded = cache.encode().unwrap();
        encoded[8] = 2;
        assert_eq!(
            BeatGridCache::decode(&encoded),
            Err(CacheError::UnsupportedVersion)
        );
    }
}
