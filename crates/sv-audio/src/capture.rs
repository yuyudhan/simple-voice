// FilePath: crates/sv-audio/src/capture.rs
//! Microphone capture on a dedicated thread.
//!
//! The cpal stream lives on a thread owned by the [`Recorder`] and is controlled over a channel,
//! so the recorder itself is `Send` and can sit in shared app state. The audio callback only
//! downmixes into a shared buffer (amortized growth, no per-call allocation, lock held for one
//! short copy); the worker thread swaps that buffer out every ~33 ms, resamples to 16 kHz,
//! converts to PCM16, and reports levels, so nothing slow ever runs on the real-time thread.
//!
//! The worker also judges the stream (see [`crate::watch`]): a device that goes away ends the
//! capture at once, keeping what was recorded, and one that delivers nothing or only digital
//! silence is reported, so a dictation never waits on a microphone that is not there.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use parking_lot::Mutex;
use sv_domain::{AppError, AppResult, CaptureProblem};

use crate::devices::{capture_problem, select_input_device};
use crate::meter::LevelMeter;
use crate::resample::{downmix_into, to_i16, Resampler};
use crate::watch::{classify, CaptureWatch, Limits, StreamFault, Verdict};
use crate::TARGET_SAMPLE_RATE;

/// Worker wake-up period, which is also the level update rate (~30 per second).
const TICK: Duration = Duration::from_millis(33);
const STREAM_OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Stream errors waiting for the worker. A failing backend may report thousands a second; the
/// first few say everything, so the rest are dropped in the callback instead of queued.
const ERROR_QUEUE: usize = 16;

type SharedBuffer = Arc<Mutex<Vec<f32>>>;
/// Receives each new stretch of 16 kHz mono PCM16 (about every 33 ms) on the capture worker
/// thread, never on the audio callback.
pub type ChunkSink = Box<dyn FnMut(&[i16]) + Send + 'static>;
/// Receives what goes wrong while recording, on the capture worker thread: a problem that ended
/// the capture (the recording stops by itself; [`Recorder::stop`] returns what was captured) or
/// one it carries on with (digital silence).
pub type ProblemSink = Box<dyn Fn(&CaptureProblem) + Send + 'static>;

/// A finished capture: 16 kHz mono PCM16.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    pub samples: Vec<i16>,
    pub duration_ms: i64,
    /// The device recorded from.
    pub device: String,
    /// What went wrong, if anything: the problem that ended the capture early, or digital
    /// silence. `None` for a capture that worked.
    pub problem: Option<CaptureProblem>,
}

/// What a capture reports besides its audio.
pub struct CaptureHooks {
    /// Receives 0..1 about 30 times per second.
    pub on_level: Box<dyn Fn(f32) + Send + 'static>,
    /// Receives every new stretch of audio while recording (live transcription, meeting files).
    pub on_chunk: Option<ChunkSink>,
    pub on_problem: Option<ProblemSink>,
}

impl std::fmt::Debug for CaptureHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureHooks")
            .field("on_chunk", &self.on_chunk.is_some())
            .field("on_problem", &self.on_problem.is_some())
            .finish_non_exhaustive()
    }
}

/// A running capture. Dropping it without [`Recorder::stop`] discards the audio and releases
/// the microphone, which is how a cancelled dictation ends.
#[derive(Debug)]
pub struct Recorder {
    stop_tx: mpsc::Sender<()>,
    worker: JoinHandle<Recording>,
    device: String,
}

impl Recorder {
    /// Opens the device (`None` prefers the built-in microphone, else the system default) and
    /// starts capturing. With `keep_samples` false the finished [`Recording`] has no samples,
    /// only its duration, so an hour-long meeting written to disk by `on_chunk` is never held in
    /// memory. Fails with the problem the user can act on when no microphone opens.
    pub fn start(
        device: Option<&str>,
        hooks: CaptureHooks,
        keep_samples: bool,
    ) -> Result<Recorder, CaptureProblem> {
        let requested = device.map(str::to_owned);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<String, CaptureProblem>>(1);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let worker = thread::Builder::new()
            .name("sv-audio-capture".to_owned())
            .spawn(move || {
                run_capture(
                    requested.as_deref(),
                    hooks,
                    keep_samples,
                    &ready_tx,
                    &stop_rx,
                )
            })
            .map_err(|error| CaptureProblem::Failed {
                device: None,
                reason: format!("the recording thread could not start ({error})"),
            })?;
        match ready_rx.recv() {
            Ok(Ok(device)) => Ok(Recorder {
                stop_tx,
                worker,
                device,
            }),
            Ok(Err(problem)) => Err(problem),
            Err(_) => Err(CaptureProblem::Failed {
                device: None,
                reason: "the recording thread stopped unexpectedly".to_owned(),
            }),
        }
    }

    /// The device this capture records from.
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Stops capturing and returns everything recorded, including the resampler tail. A capture
    /// that already ended on its own (its device went away) returns what it had by then.
    pub fn stop(self) -> AppResult<Recording> {
        // A send error means the worker already ended; joining returns what it recorded.
        let _ = self.stop_tx.send(());
        self.worker
            .join()
            .map_err(|_| AppError::Audio("The recording thread stopped unexpectedly.".to_owned()))
    }
}

/// Where the capture loop gets audio and stream errors from: the cpal stream, or a test's
/// script.
pub(crate) trait Source {
    /// Moves everything captured since the last call into `pending`.
    fn drain(&mut self, pending: &mut Vec<f32>);
    /// The next stream error reported since the last call, if any.
    fn next_fault(&mut self) -> Option<StreamFault>;
}

struct CpalSource {
    /// Dropping it stops the backend's stream thread, error reports included.
    _stream: cpal::Stream,
    shared: SharedBuffer,
    errors: mpsc::Receiver<cpal::Error>,
    device: String,
}

impl Source for CpalSource {
    fn drain(&mut self, pending: &mut Vec<f32>) {
        swap_out(&self.shared, pending);
    }

    fn next_fault(&mut self) -> Option<StreamFault> {
        self.errors
            .try_recv()
            .ok()
            .map(|error| classify(&error, &self.device))
    }
}

/// Where captured audio goes besides the level meter.
struct Sink {
    on_chunk: Option<ChunkSink>,
    keep_samples: bool,
}

fn run_capture(
    requested: Option<&str>,
    hooks: CaptureHooks,
    keep_samples: bool,
    ready_tx: &mpsc::SyncSender<Result<String, CaptureProblem>>,
    stop_rx: &mpsc::Receiver<()>,
) -> Recording {
    let (source, rate) = match open_stream(requested) {
        Ok(opened) => opened,
        Err(problem) => {
            tracing::warn!(?problem, "microphone did not open");
            let _ = ready_tx.send(Err(problem.clone()));
            return Recording {
                samples: Vec::new(),
                duration_ms: 0,
                device: requested.unwrap_or_default().to_owned(),
                problem: Some(problem),
            };
        }
    };
    let _ = ready_tx.send(Ok(source.device.clone()));
    let device = source.device.clone();
    let CaptureHooks {
        on_level,
        on_chunk,
        on_problem,
    } = hooks;
    let pipeline = Pipeline::with_sink(
        rate,
        Sink {
            on_chunk,
            keep_samples,
        },
    );
    let watch = CaptureWatch::new(&device, rate, Limits::STANDARD);
    let report = |problem: &CaptureProblem| {
        if let Some(sink) = &on_problem {
            sink(problem);
        }
    };
    let run = Run {
        pipeline,
        watch,
        device,
        on_level: &*on_level,
        report: &report,
    };
    capture_loop(source, run, stop_rx, TICK)
}

/// One capture's processing and the hooks it reports to.
struct Run<'a> {
    pipeline: Pipeline,
    watch: CaptureWatch,
    device: String,
    on_level: &'a dyn Fn(f32),
    report: &'a dyn Fn(&CaptureProblem),
}

/// Runs until a stop arrives or the stream fails. The source is dropped as soon as the loop
/// ends, before the recording is finished, so a failing device stops reporting at once.
fn capture_loop<S: Source>(
    mut source: S,
    run: Run<'_>,
    stop_rx: &mpsc::Receiver<()>,
    tick: Duration,
) -> Recording {
    let Run {
        mut pipeline,
        mut watch,
        device,
        on_level,
        report,
    } = run;
    let started = Instant::now();
    let mut pending: Vec<f32> = Vec::new();
    let mut problem: Option<CaptureProblem> = None;
    let mut ended = false;
    loop {
        let stopping = match stop_rx.recv_timeout(tick) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
            Err(RecvTimeoutError::Timeout) => false,
        };
        source.drain(&mut pending);
        pipeline.push(&pending);
        let mut verdicts = vec![watch.frames(&pending, started.elapsed())];
        pending.clear();
        while let Some(fault) = source.next_fault() {
            verdicts.push(watch.fault(fault));
        }
        for verdict in verdicts {
            match verdict {
                Verdict::Continue => {}
                Verdict::Report(found) => {
                    report(&found);
                    problem.get_or_insert(found);
                }
                Verdict::End(found) => {
                    report(&found);
                    problem = Some(found);
                    ended = true;
                }
            }
        }
        // A gone or silent-forever device ends the capture now: waiting for the stop would keep
        // the backend reporting (and spinning) and the user waiting on a microphone that is
        // not there.
        if stopping || ended {
            break;
        }
        if let Some(level) = pipeline.meter.take_latest() {
            on_level(level);
        }
    }
    // The callback may have delivered one more buffer between the last swap and the drop.
    source.drain(&mut pending);
    drop(source);
    pipeline.push(&pending);
    watch.finish();

    let mut recording = pipeline.finish();
    recording.device = device;
    recording.problem = problem;
    recording
}

fn swap_out(shared: &SharedBuffer, pending: &mut Vec<f32>) {
    let mut buffer = shared.lock();
    std::mem::swap(&mut *buffer, pending);
}

fn open_stream(requested: Option<&str>) -> Result<(CpalSource, u32), CaptureProblem> {
    let host = cpal::default_host();
    let (device, name) = select_input_device(&host, requested)?;
    let supported = device
        .default_input_config()
        .map_err(|error| capture_problem(&error, Some(&name)))?;
    let format = supported.sample_format();
    let channels = usize::from(supported.channels());
    let rate = supported.sample_rate();
    let config = supported.config();
    let shared: SharedBuffer = Arc::new(Mutex::new(Vec::with_capacity(rate as usize)));
    let buffer = Arc::clone(&shared);
    let (error_tx, errors) = mpsc::sync_channel::<cpal::Error>(ERROR_QUEUE);

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
            return Err(CaptureProblem::Failed {
                device: Some(name.clone()),
                reason: format!(
                    "“{name}” delivers {other} samples, which Simple Voice cannot read"
                ),
            })
        }
    }
    .map_err(|error| capture_problem(&error, Some(&name)))?;
    stream
        .play()
        .map_err(|error| capture_problem(&error, Some(&name)))?;
    tracing::info!(device = %name, rate, channels, %format, "recording started");
    let source = CpalSource {
        _stream: stream,
        shared,
        errors,
        device: name,
    };
    Ok((source, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    buffer: SharedBuffer,
    error_tx: mpsc::SyncSender<cpal::Error>,
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
            // A full queue already holds what the worker needs to stop the stream; a gone
            // worker means teardown. Either way this report is dropped, never queued.
            let _ = error_tx.try_send(error);
        },
        Some(STREAM_OPEN_TIMEOUT),
    )
}

/// Resampling, PCM16 conversion and metering, kept free of cpal so it is testable.
struct Pipeline {
    resampler: Resampler,
    scratch: Vec<f32>,
    /// PCM16 of the latest push, handed to the chunk sink.
    chunk: Vec<i16>,
    samples: Vec<i16>,
    /// Samples converted so far, kept or not.
    total: u64,
    meter: LevelMeter,
    sink: Sink,
}

impl std::fmt::Debug for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipeline")
            .field("total", &self.total)
            .field("keep_samples", &self.sink.keep_samples)
            .field("streams", &self.sink.on_chunk.is_some())
            .finish_non_exhaustive()
    }
}

impl Pipeline {
    fn with_sink(input_rate: u32, sink: Sink) -> Self {
        let kept = if sink.keep_samples {
            // One minute up front; longer dictations grow the buffer a handful of times.
            TARGET_SAMPLE_RATE as usize * 60
        } else {
            0
        };
        Self {
            resampler: Resampler::new(input_rate, TARGET_SAMPLE_RATE),
            scratch: Vec::with_capacity(TARGET_SAMPLE_RATE as usize / 10),
            chunk: Vec::with_capacity(TARGET_SAMPLE_RATE as usize / 10),
            samples: Vec::with_capacity(kept),
            total: 0,
            meter: LevelMeter::default(),
            sink,
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
        let millis = self.total * 1000 / u64::from(TARGET_SAMPLE_RATE);
        Recording {
            duration_ms: i64::try_from(millis).unwrap_or(i64::MAX),
            samples: self.samples,
            device: String::new(),
            problem: None,
        }
    }

    fn absorb(&mut self) {
        self.chunk.clear();
        for &sample in &self.scratch {
            self.meter.add(sample);
            self.chunk.push(to_i16(sample));
        }
        self.total += self.chunk.len() as u64;
        if self.sink.keep_samples {
            self.samples.extend_from_slice(&self.chunk);
        }
        if let Some(on_chunk) = self.sink.on_chunk.as_mut() {
            if !self.chunk.is_empty() {
                on_chunk(&self.chunk);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept_pipeline(input_rate: u32) -> Pipeline {
        Pipeline::with_sink(
            input_rate,
            Sink {
                on_chunk: None,
                keep_samples: true,
            },
        )
    }

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    /// A scripted device: delivers `chunk` frames of tone per tick until `fail_at`, then
    /// reports the failure on every look, the way ALSA repeats errno -77 until the stream is
    /// dropped (at most a full queue per tick, as the real bounded queue allows).
    struct FakeDevice {
        tick: usize,
        chunk: usize,
        fail_at: Option<usize>,
        faults_this_tick: usize,
        looks_after_failure: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Source for FakeDevice {
        fn drain(&mut self, pending: &mut Vec<f32>) {
            self.tick += 1;
            self.faults_this_tick = 0;
            if self.fail_at.is_some_and(|at| self.tick > at) {
                self.looks_after_failure.fetch_add(1, Ordering::SeqCst);
                return;
            }
            pending.extend((0..self.chunk).map(|i| 0.25 * (i as f32 * 0.12).sin()));
        }

        fn next_fault(&mut self) -> Option<StreamFault> {
            let failing = self.fail_at.is_some_and(|at| self.tick >= at);
            if !failing || self.faults_this_tick == ERROR_QUEUE {
                return None;
            }
            self.faults_this_tick += 1;
            Some(classify(
                &cpal::Error::with_message(
                    cpal::ErrorKind::BackendError,
                    "ALSA function 'snd_pcm_poll_descriptors_revents' failed with error \
                     'Unknown errno (-77)'",
                ),
                "USB Mic",
            ))
        }
    }

    impl Drop for FakeDevice {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    struct Outcome {
        recording: Recording,
        ended_by_itself: bool,
        reports: Vec<CaptureProblem>,
        looks_after_failure: usize,
        dropped: bool,
    }

    /// Runs the capture loop on `device` with a 1 ms tick. A loop that has not ended by itself
    /// within two seconds is stopped, and the outcome says so.
    fn capture(chunk: usize, fail_at: Option<usize>, limits: Limits) -> Outcome {
        let looks = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let device = FakeDevice {
            tick: 0,
            chunk,
            fail_at,
            faults_this_tick: 0,
            looks_after_failure: Arc::clone(&looks),
            dropped: Arc::clone(&dropped),
        };
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (done_tx, done_rx) = mpsc::channel::<(Recording, Vec<CaptureProblem>)>();
        let worker = thread::spawn(move || {
            let reports = Mutex::new(Vec::new());
            let report = |problem: &CaptureProblem| reports.lock().push(problem.clone());
            let run = Run {
                pipeline: kept_pipeline(16_000),
                watch: CaptureWatch::new("USB Mic", 16_000, limits),
                device: "USB Mic".to_owned(),
                on_level: &|_| {},
                report: &report,
            };
            let recording = capture_loop(device, run, &stop_rx, Duration::from_millis(1));
            let _ = done_tx.send((recording, reports.into_inner()));
        });
        let (ended_by_itself, (recording, reports)) =
            match done_rx.recv_timeout(Duration::from_secs(2)) {
                Ok(done) => (true, done),
                Err(_) => {
                    let _ = stop_tx.send(());
                    let done = done_rx
                        .recv()
                        .unwrap_or_else(|_| panic!("capture loop died"));
                    (false, done)
                }
            };
        let _ = worker.join();
        Outcome {
            recording,
            ended_by_itself,
            reports,
            looks_after_failure: looks.load(Ordering::SeqCst),
            dropped: dropped.load(Ordering::SeqCst),
        }
    }

    fn unplugged() -> CaptureProblem {
        CaptureProblem::Disconnected {
            device: "USB Mic".to_owned(),
        }
    }

    #[test]
    fn a_device_unplugged_mid_recording_ends_the_capture_and_keeps_its_audio() {
        let outcome = capture(1_600, Some(5), Limits::STANDARD);
        assert!(
            outcome.ended_by_itself,
            "the capture kept waiting on a gone device"
        );
        assert_eq!(outcome.recording.problem, Some(unplugged()));
        assert_eq!(outcome.reports, vec![unplugged()]);
        // Five ticks of 100 ms each were heard before the device went away.
        assert!(
            outcome.recording.duration_ms >= 400,
            "{}",
            outcome.recording.duration_ms
        );
        assert!(outcome.recording.samples.iter().any(|sample| *sample != 0));
        assert_eq!(outcome.recording.device, "USB Mic");
        assert!(outcome.dropped);
    }

    #[test]
    fn a_failing_device_read_does_not_spin() {
        let outcome = capture(0, Some(0), Limits::STANDARD);
        assert!(
            outcome.ended_by_itself,
            "the capture loop kept polling a failing device"
        );
        // The stream is dropped right after the look that saw the failure.
        assert!(
            outcome.looks_after_failure <= 2,
            "{} looks",
            outcome.looks_after_failure
        );
        assert!(outcome.dropped);
        assert_eq!(outcome.reports, vec![unplugged()]);
        assert_eq!(outcome.recording.duration_ms, 0);
    }

    #[test]
    fn a_device_that_never_delivers_ends_with_no_audio() {
        let limits = Limits {
            no_audio_after: Duration::from_millis(30),
            silent_after: Duration::from_secs(3),
        };
        let outcome = capture(0, None, limits);
        assert!(
            outcome.ended_by_itself,
            "the capture waited for audio forever"
        );
        let no_audio = CaptureProblem::NoAudio {
            device: "USB Mic".to_owned(),
        };
        assert_eq!(outcome.recording.problem, Some(no_audio.clone()));
        assert_eq!(outcome.reports, vec![no_audio]);
    }

    #[test]
    fn pipeline_turns_48k_into_16k_pcm16_with_duration() {
        let mut pipeline = kept_pipeline(48_000);
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

    #[test]
    fn chunks_reach_the_sink_and_unkept_audio_only_counts_its_duration() {
        let received = Arc::new(Mutex::new(Vec::<i16>::new()));
        let seen = Arc::clone(&received);
        let mut pipeline = Pipeline::with_sink(
            16_000,
            Sink {
                on_chunk: Some(Box::new(move |chunk: &[i16]| {
                    seen.lock().extend_from_slice(chunk);
                })),
                keep_samples: false,
            },
        );
        let tone: Vec<f32> = (0..32_000)
            .map(|i| 0.25 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 16_000.0).sin())
            .collect();
        for chunk in tone.chunks(533) {
            pipeline.push(chunk);
        }
        let recording = pipeline.finish();
        assert!(recording.samples.is_empty());
        assert_eq!(recording.duration_ms, 2_000);
        assert_eq!(received.lock().len(), 32_000);
    }
}
