// FilePath: src-tauri/src/features/dictation/coordinator.rs
//! The recorder state machine: start, stop, cancel, the hold/toggle shortcut semantics and the
//! watchdog. One recorder is active at a time; each finished recording is handed to its own
//! pipeline task so a new dictation can start while the previous one is still transcribing.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sv_audio::{Cue, Recorder, Recording};
use sv_domain::{AppError, DictationPhase, DictationState, PermissionStatus, Settings};
use sv_engine::FrontmostApp;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::edit::{capture_selection, edit_state, SelectionTask};
use super::learning;
use super::pipeline::{self, SessionInput};
use super::{publish, publish_message, publish_phase, Control};
use crate::features::permissions;
use crate::platform::overlay;
use crate::platform::shortcuts::{self, Binding};
use crate::state::{lock, now_ms, AppState};

/// Longest the start may wait on the engine for the frontmost app and permissions.
const START_PROBE_TIMEOUT: Duration = Duration::from_millis(150);
/// A press held at least this long on a shared hold/toggle shortcut is push-to-talk.
const HOLD_THRESHOLD: Duration = Duration::from_millis(350);
/// Shorter recordings are accidental taps.
const MIN_RECORDING_MS: i64 = 300;
/// Peak amplitude below which a recording is treated as silence (about -40 dBFS).
const SILENCE_PEAK: i32 = 330;
/// Lets the start cue finish before system output is muted.
const MUTE_DELAY: Duration = Duration::from_millis(250);
/// ~30 Hz level updates.
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

pub(crate) const MICROPHONE_DENIED: &str =
    "Simple Voice needs microphone access. Grant it in System Settings → Privacy & Security → \
     Microphone.";

struct Active {
    id: u64,
    recorder: Recorder,
    settings: Settings,
    started_at_ms: i64,
    frontmost: Option<FrontmostApp>,
    /// Output mute state before this session muted it; `None` while not (yet) muted.
    mute_previous: Arc<Mutex<Option<bool>>>,
    watchdog: JoinHandle<()>,
    /// Set for an edit: the selection read started when the edit shortcut went down.
    selection: Option<SelectionTask>,
}

struct Coordinator {
    app: AppHandle,
    active: Option<Active>,
    /// The current recording was started by pressing the hold shortcut.
    hold_started: bool,
    /// The current recording was started by pressing the edit shortcut.
    edit_started: bool,
    /// When the shared hold/toggle shortcut started the current recording.
    pressed_at: Option<Instant>,
}

pub(crate) fn spawn_coordinator(app: AppHandle, mut control: mpsc::UnboundedReceiver<Control>) {
    tauri::async_runtime::spawn(async move {
        let mut coordinator = Coordinator {
            app,
            active: None,
            hold_started: false,
            edit_started: false,
            pressed_at: None,
        };
        while let Some(message) = control.recv().await {
            coordinator.handle(message).await;
        }
    });
}

impl Coordinator {
    async fn handle(&mut self, message: Control) {
        match message {
            Control::Start => self.start(false).await,
            Control::Stop => self.stop().await,
            Control::Cancel => self.cancel().await,
            Control::Toggle => self.toggle().await,
            Control::Watchdog(id) => {
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    tracing::info!(session = id, "maximum recording length reached");
                    self.stop().await;
                }
            }
            Control::Shortcut(binding, pressed) => self.shortcut(binding, pressed).await,
            Control::ShortcutChord(binding) => self.chord(binding).await,
        }
    }

    async fn toggle(&mut self) {
        if self.active.is_some() {
            self.stop().await;
        } else {
            self.start(false).await;
        }
    }

    async fn shortcut(&mut self, binding: Binding, pressed: bool) {
        match (binding, pressed) {
            (Binding::Escape, true) => self.cancel().await,
            (Binding::Toggle, true) => self.toggle().await,
            (Binding::Hold, true) => {
                if self.active.is_none() {
                    self.start(false).await;
                    self.hold_started = self.active.is_some();
                }
            }
            (Binding::Hold, false) => {
                if self.hold_started {
                    self.stop().await;
                }
            }
            // Same accelerator for both: a tap toggles, a long press is push-to-talk.
            (Binding::Both, true) => {
                if self.active.is_some() {
                    self.stop().await;
                } else {
                    self.start(false).await;
                    self.pressed_at = self.active.as_ref().map(|_| Instant::now());
                }
            }
            (Binding::Both, false) => {
                let held = self
                    .pressed_at
                    .take()
                    .is_some_and(|at| at.elapsed() >= HOLD_THRESHOLD);
                if held {
                    self.stop().await;
                }
            }
            // Edit mode is hold-only: press to record the instruction, release to apply it.
            (Binding::Edit, true) => {
                if self.active.is_none() {
                    self.start(true).await;
                    self.edit_started = self.active.is_some();
                }
            }
            (Binding::Edit, false) => {
                if self.edit_started {
                    self.stop().await;
                }
            }
            (Binding::Escape | Binding::Toggle, false) => {}
        }
    }

    /// Discards the recording that this press of the shortcut started, if it started one.
    async fn chord(&mut self, binding: Binding) {
        let started_by_press = match binding {
            Binding::Hold => self.hold_started,
            Binding::Both => self.pressed_at.is_some(),
            Binding::Edit => self.edit_started,
            Binding::Toggle | Binding::Escape => false,
        };
        if started_by_press {
            self.cancel().await;
        }
    }

    /// Starts recording a dictation, or with `edit` the instruction for an edit of the text
    /// selected in the focused app.
    async fn start(&mut self, edit: bool) {
        if self.active.is_some() {
            return;
        }
        let app = self.app.clone();
        // Read before anything else, while the selection is surely still in place.
        let selection = edit.then(|| capture_selection(&app));
        let state = app.state::<AppState>();
        let id = state.dictation.next_session();

        let settings = match state.db() {
            Ok(db) => match db.settings().await {
                Ok(settings) => settings,
                Err(error) => return self.fail_start(id, None, &error.to_string()),
            },
            Err(error) => return self.fail_start(id, None, &error.to_string()),
        };
        // Before anything this recording pastes can reach the watched field.
        learning::prepare(&app, &settings);

        let (frontmost, permissions) = tokio::join!(
            tokio::time::timeout(START_PROBE_TIMEOUT, state.engine.frontmost_app()),
            tokio::time::timeout(START_PROBE_TIMEOUT, state.engine.permissions()),
        );
        if let Ok(Ok(permissions)) = permissions {
            permissions::observe(&app, permissions);
            if matches!(
                permissions.microphone,
                PermissionStatus::Denied | PermissionStatus::Restricted
            ) {
                return self.fail_start(id, Some(&settings), MICROPHONE_DENIED);
            }
        }
        let frontmost = frontmost.ok().and_then(Result::ok);

        let recorder = {
            let microphone = settings.microphone.clone();
            let on_level = level_emitter(app.clone());
            tauri::async_runtime::spawn_blocking(move || {
                Recorder::start(microphone.as_deref(), on_level)
            })
            .await
        };
        let recorder = match recorder {
            Ok(Ok(recorder)) => recorder,
            Ok(Err(error)) => {
                let message = format!("Could not start the microphone: {error}");
                return self.fail_start(id, Some(&settings), &message);
            }
            Err(error) => {
                let message = format!("Could not start the microphone: {error}");
                return self.fail_start(id, Some(&settings), &message);
            }
        };

        state.play(&settings, Cue::Start);
        let started_at_ms = now_ms();
        state
            .dictation
            .recording_session
            .store(id, Ordering::SeqCst);
        let mute_previous = Arc::new(Mutex::new(None));
        if settings.mute_while_dictating {
            spawn_mute(app.clone(), id, Arc::clone(&mute_previous));
        }

        let mut recording = if edit {
            edit_state(DictationPhase::Recording, id)
        } else {
            DictationState::new(DictationPhase::Recording, id)
        };
        recording.started_at = Some(started_at_ms);
        publish(&app, recording);
        shortcuts::set_escape(&app, true);

        let watchdog = {
            let app = app.clone();
            let limit = Duration::from_secs(u64::from(settings.max_recording_seconds.max(1)));
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(limit).await;
                app.state::<AppState>()
                    .dictation
                    .send(Control::Watchdog(id));
            })
        };

        self.active = Some(Active {
            id,
            recorder,
            settings,
            started_at_ms,
            frontmost,
            mute_previous,
            watchdog,
            selection,
        });
    }

    fn fail_start(&self, id: u64, settings: Option<&Settings>, message: &str) {
        tracing::warn!(session = id, message, "dictation could not start");
        if let Some(settings) = settings {
            self.app.state::<AppState>().play(settings, Cue::Error);
        }
        publish_message(&self.app, id, DictationPhase::Error, message);
    }

    /// Ends the recording span shared by stop and cancel: watchdog, Esc, mute, recorder.
    async fn finish_recording(&mut self) -> Option<Ended> {
        let Active {
            id,
            recorder,
            settings,
            started_at_ms,
            frontmost,
            mute_previous,
            watchdog,
            selection,
        } = self.active.take()?;
        self.hold_started = false;
        self.edit_started = false;
        self.pressed_at = None;
        watchdog.abort();
        let state = self.app.state::<AppState>();
        // Cleared under the slot lock so a mute landing concurrently sees the session is over
        // and restores output itself (see `spawn_mute`).
        let previous_mute = {
            let mut slot = lock(&mute_previous);
            state.dictation.recording_session.store(0, Ordering::SeqCst);
            slot.take()
        };
        shortcuts::set_escape(&self.app, false);
        let recording = tauri::async_runtime::spawn_blocking(move || recorder.stop())
            .await
            .map_err(AppError::audio)
            .and_then(|result| result);
        // Unmute before the stop cue so the cue is audible.
        if let Some(previous) = previous_mute {
            restore_mute(&self.app, previous).await;
        }
        Some(Ended {
            id,
            settings,
            started_at_ms,
            frontmost,
            recording,
            selection,
        })
    }

    async fn stop(&mut self) {
        let Some(ended) = self.finish_recording().await else {
            return;
        };
        let stopped_at = Instant::now();
        let state = self.app.state::<AppState>();
        state.play(&ended.settings, Cue::Stop);
        let recording = match ended.recording {
            Ok(recording) => recording,
            Err(error) => {
                state.play(&ended.settings, Cue::Error);
                let message = format!("Recording failed: {error}");
                return publish_message(&self.app, ended.id, DictationPhase::Error, message);
            }
        };
        if recording.duration_ms < MIN_RECORDING_MS {
            return publish_message(&self.app, ended.id, DictationPhase::Cancelled, "Too short");
        }
        if is_silent(&recording.samples) {
            return publish_message(
                &self.app,
                ended.id,
                DictationPhase::Cancelled,
                "No speech detected",
            );
        }
        let input = SessionInput {
            id: ended.id,
            recording,
            settings: ended.settings,
            started_at_ms: ended.started_at_ms,
            stopped_at,
            frontmost: ended.frontmost,
            selection: ended.selection,
        };
        tauri::async_runtime::spawn(pipeline::run_session(self.app.clone(), input));
    }

    async fn cancel(&mut self) {
        let Some(ended) = self.finish_recording().await else {
            return;
        };
        if let Err(error) = ended.recording {
            tracing::debug!(%error, "recorder error while cancelling");
        }
        publish_phase(&self.app, ended.id, DictationPhase::Cancelled);
    }
}

struct Ended {
    id: u64,
    settings: Settings,
    started_at_ms: i64,
    frontmost: Option<FrontmostApp>,
    recording: Result<Recording, AppError>,
    selection: Option<SelectionTask>,
}

fn is_silent(samples: &[i16]) -> bool {
    samples
        .iter()
        .all(|sample| i32::from(*sample).abs() < SILENCE_PEAK)
}

/// Forwards the recorder's level to the pill at most ~30 times a second.
fn level_emitter(app: AppHandle) -> Box<dyn Fn(f32) + Send + 'static> {
    let last = Mutex::new(None::<Instant>);
    Box::new(move |level: f32| {
        {
            let mut last = lock(&last);
            if last.is_some_and(|at| at.elapsed() < LEVEL_INTERVAL) {
                return;
            }
            *last = Some(Instant::now());
        }
        overlay::level(&app, level.clamp(0.0, 1.0));
    })
}

/// Mutes system output shortly after the start cue. If the session ended while the request was
/// in flight, the previous state is restored here because the stop path already looked.
fn spawn_mute(app: AppHandle, id: u64, slot: Arc<Mutex<Option<bool>>>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(MUTE_DELAY).await;
        let state = app.state::<AppState>();
        if state.dictation.recording_session.load(Ordering::SeqCst) != id {
            return;
        }
        let previous = match state.engine.set_output_muted(true).await {
            Ok(previous) => previous,
            Err(error) => {
                tracing::warn!(%error, "could not mute output");
                return;
            }
        };
        {
            let mut slot = lock(&slot);
            if state.dictation.recording_session.load(Ordering::SeqCst) == id {
                *slot = Some(previous);
                return;
            }
        }
        restore_mute(&app, previous).await;
    });
}

async fn restore_mute(app: &AppHandle, previous: bool) {
    if let Err(error) = app
        .state::<AppState>()
        .engine
        .set_output_muted(previous)
        .await
    {
        tracing::warn!(%error, "could not restore output mute");
    }
}
