// FilePath: crates/sv-audio/src/watch.rs
//! Judges a running capture: stream errors that end it, a microphone that delivers nothing or
//! only digital silence, and how often any of that reaches the log.
//!
//! A failing device can report the same error thousands of times a second (ALSA's
//! `snd_pcm_poll_descriptors_revents` with errno -77 when the PulseAudio source is gone). The
//! capture therefore stops at the first error that ends it and drops the stream, which stops
//! the backend's retry loop, and the log gets one line per kind of error, not one per report.

use std::time::Duration;

use sv_domain::health::{NO_AUDIO_AFTER_SECS, SILENT_AFTER_SECS};
use sv_domain::CaptureProblem;

use crate::devices::capture_problem;

/// How the backend's report of a stream error affects the capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamFault {
    /// An overrun or a refused real-time priority; capture goes on.
    Transient(String),
    /// The system moved the stream to another device (AirPods connected); capture goes on.
    RouteChanged(String),
    /// The stream cannot go on.
    Fatal(CaptureProblem),
}

/// Classifies a cpal stream error on `device`.
pub(crate) fn classify(error: &cpal::Error, device: &str) -> StreamFault {
    match error.kind() {
        cpal::ErrorKind::DeviceChanged => StreamFault::RouteChanged(error.to_string()),
        cpal::ErrorKind::Xrun | cpal::ErrorKind::RealtimeDenied => {
            StreamFault::Transient(error.to_string())
        }
        // A stream that ran and then fails has lost its device, whatever the backend calls it.
        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::BackendError => {
            StreamFault::Fatal(CaptureProblem::Disconnected {
                device: device.to_owned(),
            })
        }
        _ => StreamFault::Fatal(capture_problem(error, Some(device))),
    }
}

/// The thresholds a capture is judged by; tests shorten them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    pub(crate) no_audio_after: Duration,
    pub(crate) silent_after: Duration,
}

impl Limits {
    pub(crate) const STANDARD: Limits = Limits {
        no_audio_after: Duration::from_secs(NO_AUDIO_AFTER_SECS),
        silent_after: Duration::from_secs(SILENT_AFTER_SECS),
    };
}

/// What the capture loop does after a look at the stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Continue,
    /// Tell the app, keep recording (a muted microphone may be unmuted).
    Report(CaptureProblem),
    /// Stop now, keep what was captured, and tell the app why.
    End(CaptureProblem),
}

/// Watches one capture from its first tick to its last.
#[derive(Debug)]
pub(crate) struct CaptureWatch {
    device: String,
    limits: Limits,
    rate: u32,
    frames: u64,
    /// Consecutive frames that were exactly zero, up to the latest one.
    zero_run: u64,
    silence_reported: bool,
    ended: bool,
    errors: ErrorLog,
}

impl CaptureWatch {
    pub(crate) fn new(device: &str, rate: u32, limits: Limits) -> CaptureWatch {
        CaptureWatch {
            device: device.to_owned(),
            limits,
            rate: rate.max(1),
            frames: 0,
            zero_run: 0,
            silence_reported: false,
            ended: false,
            errors: ErrorLog::default(),
        }
    }

    /// Looks at the mono frames of one tick, `elapsed` after the stream started.
    pub(crate) fn frames(&mut self, mono: &[f32], elapsed: Duration) -> Verdict {
        if self.ended {
            return Verdict::Continue;
        }
        self.frames += mono.len() as u64;
        if self.frames == 0 {
            if elapsed >= self.limits.no_audio_after {
                self.ended = true;
                return Verdict::End(CaptureProblem::NoAudio {
                    device: self.device.clone(),
                });
            }
            return Verdict::Continue;
        }
        match mono.iter().rposition(|sample| *sample != 0.0) {
            Some(last_sound) => self.zero_run = (mono.len() - 1 - last_sound) as u64,
            None => self.zero_run += mono.len() as u64,
        }
        let silent_frames = self.limits.silent_after.as_millis() * u128::from(self.rate) / 1000;
        if !self.silence_reported && u128::from(self.zero_run) >= silent_frames.max(1) {
            self.silence_reported = true;
            return Verdict::Report(CaptureProblem::Silent {
                device: self.device.clone(),
            });
        }
        Verdict::Continue
    }

    /// Takes one stream error. Only the first fatal one ends the capture; every other report
    /// is counted and logged once per kind.
    pub(crate) fn fault(&mut self, fault: StreamFault) -> Verdict {
        self.errors.note(&fault);
        match fault {
            StreamFault::Fatal(problem) if !self.ended => {
                self.ended = true;
                Verdict::End(problem)
            }
            _ => Verdict::Continue,
        }
    }

    /// Logs how many repeated error reports were suppressed, once the capture is over.
    pub(crate) fn finish(&self) {
        self.errors.summary(&self.device);
    }
}

/// One log line per kind of stream error, and a count of the repeats at the end.
#[derive(Debug, Default)]
struct ErrorLog {
    last: Option<String>,
    repeats: u64,
}

impl ErrorLog {
    fn note(&mut self, fault: &StreamFault) {
        let text = match fault {
            StreamFault::Transient(text) | StreamFault::RouteChanged(text) => text.clone(),
            StreamFault::Fatal(problem) => format!("{problem:?}"),
        };
        if self.last.as_deref() == Some(text.as_str()) {
            self.repeats += 1;
            return;
        }
        match fault {
            StreamFault::Transient(text) => tracing::warn!(%text, "transient input problem"),
            StreamFault::RouteChanged(text) => {
                tracing::info!(%text, "input route changed; capture continues on the new device");
            }
            StreamFault::Fatal(problem) => tracing::warn!(?problem, "input stream failed"),
        }
        self.last = Some(text);
    }

    fn summary(&self, device: &str) {
        if self.repeats > 0 {
            tracing::info!(
                device,
                repeats = self.repeats,
                "repeated input errors not logged"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 1_000;

    fn limits() -> Limits {
        Limits {
            no_audio_after: Duration::from_millis(200),
            silent_after: Duration::from_millis(300),
        }
    }

    fn device() -> String {
        "USB Mic".to_owned()
    }

    #[test]
    fn a_stream_that_never_delivers_ends_with_no_audio() {
        let mut watch = CaptureWatch::new("USB Mic", RATE, limits());
        assert_eq!(
            watch.frames(&[], Duration::from_millis(100)),
            Verdict::Continue
        );
        assert_eq!(
            watch.frames(&[], Duration::from_millis(200)),
            Verdict::End(CaptureProblem::NoAudio { device: device() })
        );
        // Reported once; the loop has stopped by then anyway.
        assert_eq!(
            watch.frames(&[], Duration::from_millis(300)),
            Verdict::Continue
        );
    }

    #[test]
    fn digital_silence_is_reported_once_and_does_not_end_the_capture() {
        let mut watch = CaptureWatch::new("USB Mic", RATE, limits());
        let zeros = [0.0f32; 100];
        assert_eq!(watch.frames(&zeros, Duration::ZERO), Verdict::Continue);
        assert_eq!(watch.frames(&zeros, Duration::ZERO), Verdict::Continue);
        assert_eq!(
            watch.frames(&zeros, Duration::ZERO),
            Verdict::Report(CaptureProblem::Silent { device: device() })
        );
        assert_eq!(watch.frames(&zeros, Duration::ZERO), Verdict::Continue);
    }

    #[test]
    fn a_quiet_room_is_not_silence() {
        let mut watch = CaptureWatch::new("USB Mic", RATE, limits());
        let hiss: Vec<f32> = (0..100)
            .map(|i| if i % 7 == 0 { 1e-5 } else { 0.0 })
            .collect();
        for _ in 0..20 {
            assert_eq!(watch.frames(&hiss, Duration::ZERO), Verdict::Continue);
        }
        assert!(!watch.silence_reported);
    }

    #[test]
    fn the_first_fatal_error_ends_the_capture_and_repeats_are_only_counted() {
        let mut watch = CaptureWatch::new("USB Mic", RATE, limits());
        let gone = || StreamFault::Fatal(CaptureProblem::Disconnected { device: device() });
        assert_eq!(
            watch.fault(gone()),
            Verdict::End(CaptureProblem::Disconnected { device: device() })
        );
        // ALSA keeps reporting until the stream is dropped: none of it ends anything twice or
        // reaches the log again.
        for _ in 0..10_000 {
            assert_eq!(watch.fault(gone()), Verdict::Continue);
        }
        assert_eq!(watch.errors.repeats, 10_000);
        assert!(watch
            .errors
            .last
            .as_deref()
            .is_some_and(|text| text.contains("Disconnected")));
    }

    #[test]
    fn transient_errors_keep_the_capture_going() {
        let mut watch = CaptureWatch::new("USB Mic", RATE, limits());
        assert_eq!(
            watch.fault(StreamFault::Transient("xrun".to_owned())),
            Verdict::Continue
        );
        assert_eq!(
            watch.fault(StreamFault::RouteChanged("moved".to_owned())),
            Verdict::Continue
        );
        assert_eq!(watch.errors.repeats, 0);
    }

    #[test]
    fn backend_errors_after_the_stream_ran_mean_the_device_is_gone() {
        // The shape ALSA reports when the PulseAudio source vanishes (errno -77, EBADFD).
        let error = cpal::Error::with_message(
            cpal::ErrorKind::BackendError,
            "ALSA function 'snd_pcm_poll_descriptors_revents' failed with error 'Unknown errno \
             (-77)'",
        );
        assert_eq!(
            classify(&error, "default"),
            StreamFault::Fatal(CaptureProblem::Disconnected {
                device: "default".to_owned()
            })
        );
        let busy = cpal::Error::with_message(cpal::ErrorKind::DeviceBusy, "busy");
        assert_eq!(
            classify(&busy, "USB Mic"),
            StreamFault::Fatal(CaptureProblem::Busy { device: device() })
        );
        let xrun = cpal::Error::with_message(cpal::ErrorKind::Xrun, "overrun");
        assert!(matches!(
            classify(&xrun, "USB Mic"),
            StreamFault::Transient(_)
        ));
    }
}
