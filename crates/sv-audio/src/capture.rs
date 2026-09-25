// FilePath: crates/sv-audio/src/capture.rs
//! Microphone capture on a dedicated thread.
//!
//! The cpal stream lives on a thread owned by the [`Recorder`] and is controlled over a channel,
//! so the recorder itself is `Send` and can sit in shared app state. The audio callback only
//! downmixes into a shared buffer (amortized growth, no per-call allocation, lock held for one
//! short copy); the worker thread swaps that buffer out every ~33 ms, resamples to 16 kHz,
//! converts to PCM16, and reports levels, so nothing slow ever runs on the real-time thread.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use parking_lot::Mutex;
use sv_domain::{AppError, AppResult};

use crate::devices::{audio_error, select_input_device};
use crate::resample::{downmix_into, to_i16, Resampler};
use crate::TARGET_SAMPLE_RATE;

/// Worker wake-up period, which is also the level update rate (~30 per second).
const TICK: Duration = Duration::from_millis(33);
/// 33.3 ms of 16 kHz audio per level reading.
const LEVEL_WINDOW: usize = 533;
/// Quiet room tone sits below this and reads as an empty meter.
const LEVEL_FLOOR_DB: f32 = -55.0;
/// Normal speech close to a laptop mic peaks around here and reads as a full meter.
const LEVEL_CEILING_DB: f32 = -10.0;
const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(5);

type SharedBuffer = Arc<Mutex<Vec<f32>>>;

/// A finished capture: 16 kHz mono PCM16.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub samples: Vec<i16>,
    pub duration_ms: i64,
}

/// A running capture. Dropping it without [`Recorder::stop`] discards the audio and releases
/// the microphone, which is how a cancelled dictation ends.
#[derive(Debug)]
pub struct Recorder {
    stop_tx: mpsc::Sender<()>,
    worker: JoinHandle<AppResult<Recording>>,
}

impl Recorder {
    /// Opens the device (`None` prefers the built-in microphone, else the system default) and
    /// starts capturing. `on_level` receives 0..1 about 30 times per second from the capture
    /// worker thread, never from the audio callback.
    pub fn start(
        device: Option<&str>,
        on_level: Box<dyn Fn(f32) + Send + 'static>,
    ) -> AppResult<Recorder> {
        let requested = device.map(str::to_owned);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<AppResult<()>>(1);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let worker = thread::Builder::new()
            .name("sv-audio-capture".to_owned())
            .spawn(move || run_capture(requested.as_deref(), on_level, &ready_tx, &stop_rx))
            .map_err(|error| {
                AppError::Audio(format!("Could not start the recording thread: {error}"))
            })?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Recorder { stop_tx, worker }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err(join_error(worker)),
        }
    }

    /// Stops capturing and returns everything recorded, including the resampler tail.
    pub fn stop(self) -> AppResult<Recording> {
        // A send error means the worker already ended; joining reports why.
        let _ = self.stop_tx.send(());
        self.worker.join().unwrap_or_else(|_| Err(crashed()))
    }
}

fn join_error(worker: JoinHandle<AppResult<Recording>>) -> AppError {
    match worker.join() {
        Ok(Err(error)) => error,
        _ => crashed(),
    }
}

fn crashed() -> AppError {
    AppError::Audio("The recording thread stopped unexpectedly.".to_owned())
}

fn run_capture(
    requested: Option<&str>,
    on_level: Box<dyn Fn(f32) + Send + 'static>,
    ready_tx: &mpsc::SyncSender<AppResult<()>>,
    stop_rx: &mpsc::Receiver<()>,
) -> AppResult<Recording> {
    let (error_tx, error_rx) = mpsc::channel::<cpal::Error>();
    let opened = open_stream(requested, error_tx);
    let (stream, shared, input_rate) = match opened {
        Ok(parts) => parts,
        Err(error) => {
            let _ = ready_tx.send(Err(error.clone()));
            return Err(error);
        }
    };
    let _ = ready_tx.send(Ok(()));

    let mut pipeline = Pipeline::new(input_rate);
    let mut pending: Vec<f32> = Vec::with_capacity(input_rate as usize / 10);
    let mut fatal: Option<AppError> = None;
    loop {
        let stopping = match stop_rx.recv_timeout(TICK) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };
        swap_out(&shared, &mut pending);
        pipeline.push(&pending);
        pending.clear();
        while let Ok(error) = error_rx.try_recv() {
            if let Some(failure) = classify_stream_error(&error) {
                if fatal.is_none() {
                    fatal = Some(failure);
                }
            }
        }
        if fatal.is_none() {
            if let Some(level) = pipeline.meter.take_latest() {
                on_level(level);
            }
        }
        if stopping {
            break;
        }
    }
    drop(stream);
    // The callback may have delivered one more buffer between the last swap and the drop.
    swap_out(&shared, &mut pending);
    pipeline.push(&pending);

    match fatal {
        Some(error) => Err(error),
        None => Ok(pipeline.finish()),
    }
}

fn swap_out(shared: &SharedBuffer, pending: &mut Vec<f32>) {
    let mut buffer = shared.lock();
    std::mem::swap(&mut *buffer, pending);
}

/// Returns the error to report for failures that end the recording; transient ones are logged.
fn classify_stream_error(error: &cpal::Error) -> Option<AppError> {
    match error.kind() {
        cpal::ErrorKind::DeviceChanged => {
            tracing::info!(%error, "input route changed; capture continues on the new device");
            None
        }
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => {
            tracing::warn!(%error, "transient input stream problem");
            None
        }
        _ => {
            tracing::warn!(%error, "input stream failed");
            Some(audio_error(error))
        }
    }
}

fn open_stream(
    requested: Option<&str>,
    error_tx: mpsc::Sender<cpal::Error>,
) -> AppResult<(cpal::Stream, SharedBuffer, u32)> {
    let host = cpal::default_host();
    let (device, name) = select_input_device(&host, requested)?;
    let supported = device
        .default_input_config()
        .map_err(|error| audio_error(&error))?;
    let format = supported.sample_format();
    let channels = usize::from(supported.channels());
    let rate = supported.sample_rate();
    let config = supported.config();
    let shared: SharedBuffer = Arc::new(Mutex::new(Vec::with_capacity(rate as usize)));
    let buffer = Arc::clone(&shared);

    let stream = match format {
        SampleFormat::I8 => build::<i8>(&device, config, channels, buffer, error_tx),
        SampleFormat::I16 => build::<i16>(&device, config, channels, buffer, error_tx),
        SampleFormat::I32 => build::<i32>(&device, config, channels, buffer, error_tx),
        SampleFormat::I64 => build::<i64>(&device, config, channels, buffer, error_tx),
        SampleFormat::U8 => build::<u8>(&device, config, channels, buffer, error_tx),
        SampleFormat::U16 => build::<u16>(&device, config, channels, buffer, error_tx),
        SampleFormat::U32 => build::<u32>(&device, config, channels, buffer, error_tx),
        SampleFormat::U64 => build::<u64>(&device, config, channels, buffer, error_tx),
        SampleFormat::F32 => build::<f32>(&device, config, channels, buffer, error_tx),
        SampleFormat::F64 => build::<f64>(&device, config, channels, buffer, error_tx),
        other => {
            return Err(AppError::Audio(format!(
                "The microphone \"{name}\" delivers {other} samples, which Simple Voice cannot \
                 read. Pick another microphone in Settings."
            )))
        }
    }
    .map_err(|error| audio_error(&error))?;
    stream.play().map_err(|error| audio_error(&error))?;
    tracing::info!(device = %name, rate, channels, %format, "recording started");
    Ok((stream, shared, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    buffer: SharedBuffer,
    error_tx: mpsc::Sender<cpal::Error>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + 'static,
    f32: FromSample<T>,
{
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut mono = buffer.lock();
            downmix_into(
                data,
                channels,
                |sample| sample.to_sample::<f32>(),
                &mut mono,
            );
        },
        move |error| {
            // The worker may already be gone during teardown; nothing is left to tell.
            let _ = error_tx.send(error);
        },
        Some(STREAM_OPEN_TIMEOUT),
    )
}

/// Resampling, PCM16 conversion and metering, kept free of cpal so it is testable.
#[derive(Debug)]
struct Pipeline {
    resampler: Resampler,
    scratch: Vec<f32>,
    samples: Vec<i16>,
    meter: LevelMeter,
}

impl Pipeline {
    fn new(input_rate: u32) -> Self {
        Self {
            resampler: Resampler::new(input_rate, TARGET_SAMPLE_RATE),
            scratch: Vec::with_capacity(TARGET_SAMPLE_RATE as usize / 10),
            // One minute up front; longer dictations grow the buffer a handful of times.
            samples: Vec::with_capacity(TARGET_SAMPLE_RATE as usize * 60),
            meter: LevelMeter::default(),
        }
    }

    fn push(&mut self, mono: &[f32]) {
        self.scratch.clear();
        self.resampler.process(mono, &mut self.scratch);
        self.absorb();
    }

    fn finish(mut self) -> Recording {
        self.scratch.clear();
        self.resampler.flush(&mut self.scratch);
        self.absorb();
        let millis = self.samples.len() as u64 * 1000 / u64::from(TARGET_SAMPLE_RATE);
        Recording {
            duration_ms: i64::try_from(millis).unwrap_or(i64::MAX),
            samples: self.samples,
        }
    }

    fn absorb(&mut self) {
        for &sample in &self.scratch {
            self.meter.add(sample);
            self.samples.push(to_i16(sample));
        }
    }
}

#[derive(Debug, Default)]
struct LevelMeter {
    sum_squares: f64,
    count: usize,
    latest: Option<f32>,
}

impl LevelMeter {
    fn add(&mut self, sample: f32) {
        self.sum_squares += f64::from(sample) * f64::from(sample);
        self.count += 1;
        if self.count >= LEVEL_WINDOW {
            let rms = (self.sum_squares / self.count as f64).sqrt() as f32;
            self.latest = Some(level_from_rms(rms));
            self.sum_squares = 0.0;
            self.count = 0;
        }
    }

    fn take_latest(&mut self) -> Option<f32> {
        self.latest.take()
    }
}

/// Maps RMS amplitude to a 0..1 meter value on a dB scale, which matches perceived loudness:
/// linear RMS would leave the meter nearly empty for normal speech.
fn level_from_rms(rms: f32) -> f32 {
    if rms.is_nan() || rms <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * rms.log10();
    ((db - LEVEL_FLOOR_DB) / (LEVEL_CEILING_DB - LEVEL_FLOOR_DB)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_mapping_is_bounded() {
        assert_eq!(level_from_rms(0.0), 0.0);
        assert_eq!(level_from_rms(-1.0), 0.0);
        assert_eq!(level_from_rms(f32::NAN), 0.0);
        assert_eq!(level_from_rms(1e-6), 0.0);
        assert_eq!(level_from_rms(1.0), 1.0);
        assert_eq!(level_from_rms(10.0), 1.0);
        let mid = level_from_rms(10f32.powf(-32.5 / 20.0));
        assert!((mid - 0.5).abs() < 1e-3, "{mid}");
    }

    #[test]
    fn level_mapping_is_monotonic() {
        let quiet = level_from_rms(0.003);
        let speech = level_from_rms(0.05);
        let loud = level_from_rms(0.2);
        assert!(quiet < speech && speech < loud);
    }

    #[test]
    fn meter_reports_once_per_window() {
        let mut meter = LevelMeter::default();
        for _ in 0..LEVEL_WINDOW - 1 {
            meter.add(0.1);
        }
        assert_eq!(meter.take_latest(), None);
        meter.add(0.1);
        let level = meter.take_latest();
        assert_eq!(level, Some(level_from_rms(0.1)));
        assert_eq!(meter.take_latest(), None);
    }

    #[test]
    fn pipeline_turns_48k_into_16k_pcm16_with_duration() {
        let mut pipeline = Pipeline::new(48_000);
        let tone: Vec<f32> = (0..48_000)
            .map(|i| 0.25 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 48_000.0).sin())
            .collect();
        for chunk in tone.chunks(512) {
            pipeline.push(chunk);
        }
        assert!(pipeline
            .meter
            .take_latest()
            .is_some_and(|level| level > 0.5));
        let recording = pipeline.finish();
        assert_eq!(recording.samples.len(), 16_000);
        assert_eq!(recording.duration_ms, 1_000);
        let peak = recording
            .samples
            .iter()
            .map(|s| s.unsigned_abs())
            .max()
            .unwrap_or(0);
        assert!((8_000..8_600).contains(&peak), "{peak}");
    }
}
