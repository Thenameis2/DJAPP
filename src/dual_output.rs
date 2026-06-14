use std::{
    error::Error,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DualOutputSample {
    pub elapsed_seconds: u64,
    pub master_frames: u64,
    pub cue_frames: u64,
    pub relative_frames: i64,
    pub drift_from_baseline_frames: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DualOutputReport {
    pub master_name: String,
    pub cue_name: String,
    pub sample_rate: u32,
    pub duration_seconds: u64,
    pub master_callbacks: u64,
    pub cue_callbacks: u64,
    pub master_frames: u64,
    pub cue_frames: u64,
    pub master_stream_errors: u64,
    pub cue_stream_errors: u64,
    pub final_drift_frames: i64,
    pub maximum_absolute_drift_frames: u64,
    pub samples: Vec<DualOutputSample>,
}

impl DualOutputReport {
    pub fn summary(&self) -> String {
        format!(
            "Dual-output report: master={}, cue={}, rate={}Hz, duration={}s, master_callbacks={}, cue_callbacks={}, master_frames={}, cue_frames={}, final_drift_frames={}, max_abs_drift_frames={}, master_errors={}, cue_errors={}",
            self.master_name,
            self.cue_name,
            self.sample_rate,
            self.duration_seconds,
            self.master_callbacks,
            self.cue_callbacks,
            self.master_frames,
            self.cue_frames,
            self.final_drift_frames,
            self.maximum_absolute_drift_frames,
            self.master_stream_errors,
            self.cue_stream_errors,
        )
    }
}

#[derive(Default)]
struct StreamMetrics {
    callbacks: AtomicU64,
    frames: AtomicU64,
    errors: AtomicU64,
}

pub fn run_dual_output_probe(
    master_name: &str,
    cue_name: &str,
    duration: Duration,
) -> Result<DualOutputReport, Box<dyn Error>> {
    if duration.is_zero() {
        return Err("dual-output duration must be greater than zero".into());
    }
    let host = cpal::default_host();
    let master = find_output_device(&host, master_name)?;
    let cue = find_output_device(&host, cue_name)?;
    let master_label = master.description()?.name().to_string();
    let cue_label = cue.description()?.name().to_string();
    if master == cue {
        return Err("master and cue must use different output devices".into());
    }

    let master_supported = master.default_output_config()?;
    let cue_supported = cue.default_output_config()?;
    if master_supported.sample_rate() != cue_supported.sample_rate() {
        return Err(format!(
            "dual-output spike requires matching nominal rates; master={}Hz cue={}Hz",
            master_supported.sample_rate(),
            cue_supported.sample_rate()
        )
        .into());
    }
    let sample_rate = master_supported.sample_rate();
    let master_format = master_supported.sample_format();
    let cue_format = cue_supported.sample_format();
    let master_config: StreamConfig = master_supported.into();
    let cue_config: StreamConfig = cue_supported.into();
    let master_metrics = Arc::new(StreamMetrics::default());
    let cue_metrics = Arc::new(StreamMetrics::default());
    let master_stream = build_silent_stream(
        &master,
        &master_config,
        master_format,
        Arc::clone(&master_metrics),
    )?;
    let cue_stream = build_silent_stream(&cue, &cue_config, cue_format, Arc::clone(&cue_metrics))?;

    master_stream.play()?;
    cue_stream.play()?;
    thread::sleep(Duration::from_secs(2));
    let baseline = relative_frames(&master_metrics, &cue_metrics);
    let duration_seconds = duration.as_secs();
    let mut samples = Vec::with_capacity(duration_seconds as usize);
    let mut maximum_absolute_drift_frames = 0;
    for elapsed_seconds in 1..=duration_seconds {
        thread::sleep(Duration::from_secs(1));
        let master_frames = master_metrics.frames.load(Ordering::Relaxed);
        let cue_frames = cue_metrics.frames.load(Ordering::Relaxed);
        let relative_frames = signed_difference(master_frames, cue_frames);
        let drift_from_baseline_frames = relative_frames - baseline;
        maximum_absolute_drift_frames =
            maximum_absolute_drift_frames.max(drift_from_baseline_frames.unsigned_abs());
        samples.push(DualOutputSample {
            elapsed_seconds,
            master_frames,
            cue_frames,
            relative_frames,
            drift_from_baseline_frames,
        });
    }

    let final_drift_frames = samples
        .last()
        .map(|sample| sample.drift_from_baseline_frames)
        .unwrap_or(0);
    drop(master_stream);
    drop(cue_stream);
    let (master_frames, cue_frames) = samples
        .last()
        .map(|sample| (sample.master_frames, sample.cue_frames))
        .unwrap_or((0, 0));
    Ok(DualOutputReport {
        master_name: master_label,
        cue_name: cue_label,
        sample_rate,
        duration_seconds,
        master_callbacks: master_metrics.callbacks.load(Ordering::Relaxed),
        cue_callbacks: cue_metrics.callbacks.load(Ordering::Relaxed),
        master_frames,
        cue_frames,
        master_stream_errors: master_metrics.errors.load(Ordering::Relaxed),
        cue_stream_errors: cue_metrics.errors.load(Ordering::Relaxed),
        final_drift_frames,
        maximum_absolute_drift_frames,
        samples,
    })
}

fn find_output_device(host: &cpal::Host, query: &str) -> Result<Device, Box<dyn Error>> {
    let query_lower = query.to_lowercase();
    let mut matches =
        host.output_devices()?
            .filter(|device| {
                device.description().is_ok_and(|description| {
                    description.name().to_lowercase().contains(&query_lower)
                }) || device.id().is_ok_and(|id| id.to_string() == query)
            })
            .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!("no output device matches: {query}").into()),
        count => Err(format!("{count} output devices match: {query}").into()),
    }
}

fn build_silent_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    metrics: Arc<StreamMetrics>,
) -> Result<Stream, Box<dyn Error>> {
    Ok(match format {
        SampleFormat::F32 => build_silent_stream_typed::<f32>(device, config, metrics)?,
        SampleFormat::F64 => build_silent_stream_typed::<f64>(device, config, metrics)?,
        SampleFormat::I8 => build_silent_stream_typed::<i8>(device, config, metrics)?,
        SampleFormat::I16 => build_silent_stream_typed::<i16>(device, config, metrics)?,
        SampleFormat::I24 => build_silent_stream_typed::<cpal::I24>(device, config, metrics)?,
        SampleFormat::I32 => build_silent_stream_typed::<i32>(device, config, metrics)?,
        SampleFormat::I64 => build_silent_stream_typed::<i64>(device, config, metrics)?,
        SampleFormat::U8 => build_silent_stream_typed::<u8>(device, config, metrics)?,
        SampleFormat::U16 => build_silent_stream_typed::<u16>(device, config, metrics)?,
        SampleFormat::U32 => build_silent_stream_typed::<u32>(device, config, metrics)?,
        SampleFormat::U64 => build_silent_stream_typed::<u64>(device, config, metrics)?,
        other => return Err(format!("unsupported output sample format: {other:?}").into()),
    })
}

fn build_silent_stream_typed<T>(
    device: &Device,
    config: &StreamConfig,
    metrics: Arc<StreamMetrics>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = u64::from(config.channels);
    let error_metrics = Arc::clone(&metrics);
    device.build_output_stream(
        *config,
        move |output: &mut [T], _| {
            output.fill(T::from_sample(0.0));
            metrics.callbacks.fetch_add(1, Ordering::Relaxed);
            metrics
                .frames
                .fetch_add(output.len() as u64 / channels, Ordering::Relaxed);
        },
        move |_error| {
            error_metrics.errors.fetch_add(1, Ordering::Relaxed);
        },
        None,
    )
}

fn relative_frames(master: &StreamMetrics, cue: &StreamMetrics) -> i64 {
    signed_difference(
        master.frames.load(Ordering::Relaxed),
        cue.frames.load(Ordering::Relaxed),
    )
}

fn signed_difference(left: u64, right: u64) -> i64 {
    i128::from(left)
        .saturating_sub(i128::from(right))
        .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_frame_difference_handles_both_directions() {
        assert_eq!(signed_difference(10, 4), 6);
        assert_eq!(signed_difference(4, 10), -6);
    }

    #[test]
    fn report_summary_contains_drift_and_health() {
        let report = DualOutputReport {
            master_name: "Speakers".to_string(),
            cue_name: "Headphones".to_string(),
            sample_rate: 44_100,
            duration_seconds: 30,
            master_callbacks: 100,
            cue_callbacks: 110,
            master_frames: 1_323_000,
            cue_frames: 1_323_010,
            master_stream_errors: 0,
            cue_stream_errors: 0,
            final_drift_frames: -10,
            maximum_absolute_drift_frames: 512,
            samples: Vec::new(),
        };
        assert!(report.summary().contains("final_drift_frames=-10"));
        assert!(report.summary().contains("max_abs_drift_frames=512"));
    }
}
