// FilePath: src-tauri/src/features/dictation/live.rs
//! Live transcription while recording. When the voice model streams (the helper reports
//! `streaming` for Parakeet TDT, Parakeet Flash and Apple Speech; Whisper does not), the
//! recorder hands every stretch of audio to a bounded queue, a feeder task forwards it to the
//! helper's stream in ~200 ms batches, and the helper's partial hypotheses reach the UI
//! (`dictation-partial`) and the pill. The stream is display only: on stop it is cancelled and
//! the pasted text comes from the normal transcription of the whole recording, so the result is
//! as accurate as without live transcript and never repeats a segment. A stream that fails,
//! overflows its queue or dies with a helper restart only stops the live display.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sv_audio::ChunkSink;
use sv_domain::{DictationPartial, Settings};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use crate::events;
use crate::features::models::{active_model, engine_language};
use crate::state::{lock, AppState};

/// Queue depth in recorder chunks (~33 ms each): about 8 s of audio waiting for the helper to
/// start the stream. More than that and live transcription is abandoned for this recording.
const QUEUE_CHUNKS: usize = 256;
/// Samples per `stream_audio` line: 200 ms at 16 kHz.
const BATCH_SAMPLES: usize = 3_200;
/// Longest the helper may take to load the model and open the stream.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// At most this many partial updates per second reach the UI.
const PARTIAL_INTERVAL: Duration = Duration::from_millis(120);

/// The stream of the recording in progress, as the helper's events see it.
#[derive(Debug, Default)]
pub(crate) struct LiveState {
    current: Mutex<Option<Current>>,
}

#[derive(Debug)]
struct Current {
    session_id: u64,
    stream_id: Option<u64>,
    failed: bool,
    last_partial: Option<Instant>,
}

impl LiveState {
    fn begin(&self, session_id: u64) {
        *lock(&self.current) = Some(Current {
            session_id,
            stream_id: None,
            failed: false,
            last_partial: None,
        });
    }

    fn attach(&self, session_id: u64, stream_id: u64) -> bool {
        match lock(&self.current).as_mut() {
            Some(current) if current.session_id == session_id => {
                current.stream_id = Some(stream_id);
                true
            }
            _ => false,
        }
    }

    fn failed(&self, session_id: u64) -> bool {
        lock(&self.current)
            .as_ref()
            .is_none_or(|current| current.session_id != session_id || current.failed)
    }

    fn end(&self, session_id: u64) {
        let mut current = lock(&self.current);
        if current
            .as_ref()
            .is_some_and(|current| current.session_id == session_id)
        {
            *current = None;
        }
    }
}

/// A recording's live stream: the feeder task and the model it streams with.
#[derive(Debug)]
pub(crate) struct LiveSession {
    session_id: u64,
    overflowed: Arc<AtomicBool>,
    feeder: JoinHandle<()>,
}

/// The model to stream with, when live transcript is on: the model the dictation would run
/// first. Whether it streams is the helper's call; `stream_start` refuses models that do not.
pub(crate) async fn live_model(state: &AppState, settings: &Settings) -> Option<String> {
    if !settings.live_transcript {
        return None;
    }
    active_model(state, settings).await
}

/// Starts the feeder for `session_id` and returns the chunk sink the recorder writes into. The
/// feeder picks the model itself (readiness checks never delay the recording start); audio
/// waits in the queue meanwhile.
pub(crate) fn start(
    app: &AppHandle,
    session_id: u64,
    settings: &Settings,
) -> (LiveSession, ChunkSink) {
    let (sender, receiver) = mpsc::channel::<Vec<i16>>(QUEUE_CHUNKS);
    let overflowed = Arc::new(AtomicBool::new(false));
    let sink: ChunkSink = {
        let overflowed = Arc::clone(&overflowed);
        Box::new(move |chunk: &[i16]| {
            if overflowed.load(Ordering::Relaxed) {
                return;
            }
            // A full queue means the helper fell behind: dropping audio would corrupt the
            // stream, so the stream is abandoned and the batch transcription takes over.
            if sender.try_send(chunk.to_vec()).is_err() {
                overflowed.store(true, Ordering::Relaxed);
            }
        })
    };
    app.state::<AppState>().dictation.live.begin(session_id);
    let feeder = tauri::async_runtime::spawn(feed(
        app.clone(),
        session_id,
        settings.clone(),
        receiver,
        Arc::clone(&overflowed),
    ));
    let session = LiveSession {
        session_id,
        overflowed,
        feeder,
    };
    (session, sink)
}

async fn feed(
    app: AppHandle,
    session_id: u64,
    settings: Settings,
    mut receiver: mpsc::Receiver<Vec<i16>>,
    overflowed: Arc<AtomicBool>,
) {
    let state = app.state::<AppState>();
    let Some(model) = live_model(&state, &settings).await else {
        tracing::info!("no ready voice model for the live transcript");
        return;
    };
    let language = engine_language(&settings, &model).map(str::to_owned);
    let started = tokio::time::timeout(
        START_TIMEOUT,
        state.engine.stream_start(&model, language.as_deref(), true),
    )
    .await;
    let stream_id = match started {
        Ok(Ok(stream_id)) => stream_id,
        Ok(Err(error)) => {
            tracing::info!(%error, model, "live transcript unavailable");
            return;
        }
        Err(_) => {
            tracing::info!(model, "live transcript did not start in time");
            return;
        }
    };
    if !state.dictation.live.attach(session_id, stream_id) {
        cancel_stream(&state, stream_id).await;
        return;
    }
    let mut batch: Vec<i16> = Vec::with_capacity(BATCH_SAMPLES * 2);
    while let Some(chunk) = receiver.recv().await {
        if overflowed.load(Ordering::Relaxed) || state.dictation.live.failed(session_id) {
            tracing::info!(session = session_id, "live transcript abandoned");
            cancel_stream(&state, stream_id).await;
            return;
        }
        batch.extend_from_slice(&chunk);
        if batch.len() >= BATCH_SAMPLES {
            if let Err(error) = state.engine.stream_audio(stream_id, &batch) {
                tracing::info!(%error, "live transcript lost the helper");
                return;
            }
            batch.clear();
        }
    }
    if !batch.is_empty() {
        if let Err(error) = state.engine.stream_audio(stream_id, &batch) {
            tracing::info!(%error, "live transcript lost the helper");
        }
    }
}

impl LiveSession {
    /// Ends the stream: the recording stopped (the pasted text comes from transcribing the whole
    /// recording, which is more accurate than the stream's sliding windows) or was cancelled.
    pub(crate) async fn end(self, app: &AppHandle) {
        self.overflowed.store(true, Ordering::Relaxed);
        self.feeder.abort();
        let state = app.state::<AppState>();
        let stream_id = lock(&state.dictation.live.current)
            .as_ref()
            .filter(|current| current.session_id == self.session_id)
            .and_then(|current| current.stream_id);
        state.dictation.live.end(self.session_id);
        if let Some(stream_id) = stream_id {
            cancel_stream(&state, stream_id).await;
        }
    }
}

async fn cancel_stream(state: &AppState, stream_id: u64) {
    if let Err(error) = state.engine.stream_cancel(stream_id).await {
        tracing::debug!(%error, "live transcript cancel failed");
    }
}

/// A new hypothesis from the helper: shown in the UI while its recording is the current one.
pub(crate) fn on_partial(app: &AppHandle, stream_id: u64, text: &str, stable: &str) {
    let state = app.state::<AppState>();
    let session_id = {
        let mut current = lock(&state.dictation.live.current);
        let Some(current) = current
            .as_mut()
            .filter(|current| current.stream_id == Some(stream_id) && !current.failed)
        else {
            return;
        };
        if current
            .last_partial
            .is_some_and(|at| at.elapsed() < PARTIAL_INTERVAL)
        {
            return;
        }
        current.last_partial = Some(Instant::now());
        current.session_id
    };
    events::emit(
        app,
        events::DICTATION_PARTIAL,
        partial(session_id, text, stable),
    );
}

/// The helper gave up on the stream (recogniser error or backpressure); the recording falls
/// back to batch transcription when it stops.
pub(crate) fn on_failed(app: &AppHandle, stream_id: u64, reason: &str) {
    let state = app.state::<AppState>();
    let mut current = lock(&state.dictation.live.current);
    if let Some(current) = current
        .as_mut()
        .filter(|current| current.stream_id == Some(stream_id))
    {
        tracing::info!(
            reason,
            session = current.session_id,
            "live transcript failed"
        );
        current.failed = true;
    }
}

/// Splits the helper's hypothesis into the committed part and the part that may still change.
/// `stable` is a prefix of `text` by contract; anything else is treated as all tentative.
fn partial(session_id: u64, text: &str, stable: &str) -> DictationPartial {
    match text.strip_prefix(stable) {
        Some(rest) => DictationPartial {
            session_id,
            text: stable.to_owned(),
            tentative: rest.to_owned(),
        },
        None => DictationPartial {
            session_id,
            text: String::new(),
            tentative: text.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_splits_committed_from_tentative_text() {
        let split = partial(4, "move the standup to ten", "move the standup");
        assert_eq!(split.session_id, 4);
        assert_eq!(split.text, "move the standup");
        assert_eq!(split.tentative, " to ten");
        // A hypothesis that does not extend its stable part is never shown as committed.
        let odd = partial(4, "moved", "move the");
        assert_eq!((odd.text.as_str(), odd.tentative.as_str()), ("", "moved"));
    }

    #[test]
    fn live_state_tracks_only_the_current_session() {
        let live = LiveState::default();
        assert!(live.failed(1));
        live.begin(1);
        assert!(!live.failed(1));
        assert!(live.attach(1, 77));
        assert!(!live.attach(2, 78));
        live.end(2);
        assert!(!live.failed(1));
        live.end(1);
        assert!(live.failed(1));
    }
}
