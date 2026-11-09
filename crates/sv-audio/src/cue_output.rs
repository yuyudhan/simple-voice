// FilePath: crates/sv-audio/src/cue_output.rs
//! The output stream cues play on, and what happens when it fails.
//!
//! A failing output stream can report the same error thousands of times a second: on a Linux
//! PC whose PulseAudio sink went away, ALSA's `snd_pcm_poll_descriptors_revents` fails with
//! errno -77 on every poll, and rodio's default handler printed each one (4.7 million lines a
//! day on itlinux1). Here the stream's error handler only flags the failure, once; the player
//! drops the stream, which ends the backend's loop, and opens a new one for a later cue no
//! sooner than an escalating pause allows. The log gets one line when output fails and one
//! when it is back.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rodio::cpal::StreamError;

/// Pauses before opening the output again after a failure, escalating to a cap of ten minutes.
const REOPEN_AFTER: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
    Duration::from_secs(600),
];

/// Tells the player thread, once per stream, that the stream failed.
#[derive(Debug)]
pub(crate) struct FailureSignal<M> {
    raised: Arc<AtomicBool>,
    player: mpsc::Sender<M>,
    message: fn() -> M,
}

// By hand: a derive would require `M: Clone`, which the message need not be.
impl<M> Clone for FailureSignal<M> {
    fn clone(&self) -> Self {
        FailureSignal {
            raised: Arc::clone(&self.raised),
            player: self.player.clone(),
            message: self.message,
        }
    }
}

impl<M> FailureSignal<M> {
    pub(crate) fn new(player: mpsc::Sender<M>, message: fn() -> M) -> FailureSignal<M> {
        FailureSignal {
            raised: Arc::new(AtomicBool::new(false)),
            player,
            message,
        }
    }

    /// Called from the stream's error handler, possibly thousands of times a second.
    pub(crate) fn on_error(&self, error: &StreamError) {
        if matches!(error, StreamError::BufferUnderrun) {
            return;
        }
        if !self.raised.swap(true, Ordering::SeqCst) {
            // The player may have quit with the app; nobody is left to tell.
            let _ = self.player.send((self.message)());
        }
    }
}

/// When the output may be opened again.
#[derive(Debug, Default)]
pub(crate) struct Reopen {
    failures: usize,
    not_before: Option<Instant>,
}

impl Reopen {
    /// Records a failure at `now` and returns the pause before the next attempt.
    pub(crate) fn failed(&mut self, now: Instant) -> Duration {
        let pause = REOPEN_AFTER[self.failures.min(REOPEN_AFTER.len() - 1)];
        self.failures = self.failures.saturating_add(1);
        self.not_before = Some(now + pause);
        pause
    }

    pub(crate) fn may_open(&self, now: Instant) -> bool {
        self.not_before.is_none_or(|at| now >= at)
    }

    /// Whether the output had failed before this successful open.
    pub(crate) fn opened(&mut self) -> bool {
        let recovered = self.failures > 0;
        self.failures = 0;
        self.not_before = None;
        recovered
    }
}

/// The default output device's mixer, opened lazily and dropped on failure.
pub(crate) struct Output<M> {
    sink: Option<rodio::MixerDeviceSink>,
    reopen: Reopen,
    player: mpsc::Sender<M>,
    failed: fn() -> M,
}

impl<M: Send + 'static> Output<M> {
    pub(crate) fn new(player: mpsc::Sender<M>, failed: fn() -> M) -> Output<M> {
        Output {
            sink: None,
            reopen: Reopen::default(),
            player,
            failed,
        }
    }

    /// The mixer to play on, opening the output when it is closed and the pause after the last
    /// failure is over.
    pub(crate) fn mixer(&mut self, now: Instant) -> Option<&rodio::mixer::Mixer> {
        if self.sink.is_none() && self.reopen.may_open(now) {
            self.open(now);
        }
        self.sink.as_ref().map(rodio::MixerDeviceSink::mixer)
    }

    /// The stream reported a failure: drop it now, so the backend stops reporting.
    pub(crate) fn failed(&mut self, now: Instant) {
        if self.sink.take().is_some() {
            let pause = self.reopen.failed(now);
            tracing::warn!(?pause, "audio output failed; cues paused until it is back");
        }
    }

    fn open(&mut self, now: Instant) {
        let signal = FailureSignal::new(self.player.clone(), self.failed);
        let opened = rodio::DeviceSinkBuilder::from_default_device().and_then(|builder| {
            builder
                .with_error_callback(move |error: StreamError| signal.on_error(&error))
                .open_stream()
        });
        match opened {
            Ok(mut sink) => {
                // The sink is only dropped on failure or when the app quits.
                sink.log_on_drop(false);
                if self.reopen.opened() {
                    tracing::info!("audio output is back; cues play again");
                }
                self.sink = Some(sink);
            }
            Err(error) => {
                let first = self.reopen.failed(now) == REOPEN_AFTER[0];
                if first {
                    tracing::warn!(%error, "no audio output for cues");
                } else {
                    tracing::debug!(%error, "audio output still unavailable");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Failed;

    #[test]
    fn a_failing_stream_signals_the_player_once() {
        let (player, inbox) = mpsc::channel();
        let signal = FailureSignal::new(player, || Failed);
        let error = StreamError::DeviceNotAvailable;
        for _ in 0..10_000 {
            signal.clone().on_error(&error);
        }
        assert_eq!(inbox.try_recv().ok(), Some(Failed));
        assert!(
            inbox.try_recv().is_err(),
            "the player was told more than once"
        );
    }

    #[test]
    fn an_underrun_is_not_a_failure() {
        let (player, inbox) = mpsc::channel();
        let signal = FailureSignal::new(player, || Failed);
        signal.on_error(&StreamError::BufferUnderrun);
        assert!(inbox.try_recv().is_err());
    }

    #[test]
    fn a_failed_output_is_not_reopened_in_a_loop() {
        let start = Instant::now();
        let mut reopen = Reopen::default();
        assert!(reopen.may_open(start));
        let pauses: Vec<u64> = (0..7).map(|_| reopen.failed(start).as_secs()).collect();
        assert_eq!(pauses, vec![1, 5, 30, 120, 600, 600, 600]);
        // Every cue in between is skipped without touching the device.
        assert!(!reopen.may_open(start + Duration::from_secs(599)));
        assert!(reopen.may_open(start + Duration::from_secs(600)));
        assert!(reopen.opened());
        assert!(reopen.may_open(start));
        assert!(!reopen.opened());
    }
}
