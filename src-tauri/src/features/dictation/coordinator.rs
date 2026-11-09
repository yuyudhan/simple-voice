// FilePath: src-tauri/src/features/dictation/coordinator.rs
//! The recorder state machine: start, stop, cancel, the hold/toggle/edit shortcut semantics and
//! watchdog. One recorder is active at a time; each finished recording is handed to its own
//! pipeline task so a new dictation can start while the previous one is still transcribing.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use sv_audio::{CaptureHooks, Cue, Recorder, Recording};
use sv_domain::{AppError, DictationPhase, DictationState, Issue, Platform, Settings, StyleSource};
use sv_engine::FrontmostApp;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;

use super::capture_hooks::{level_emitter, problem_reporter};
use super::context::{self, ContextTask};
use super::edit::{capture_selection, edit_state, SelectionTask};
use super::learning;
use super::live::{self, LiveSession};
use super::pipeline::{self, SessionInput};
use super::{publish, publish_message, publish_phase, Control};
use crate::features::{health, llm, permissions};
use crate::platform::output_mute::{self, Muting};
use crate::platform::shortcuts::{self, Binding};
use crate::platform::windows;
use crate::state::{now_ms, AppState};

/// Longest the start may wait on the engine for the frontmost app and permissions.
const START_PROBE_TIMEOUT: Duration = Duration::from_millis(150);
/// A press held at least this long on a shared hold/toggle shortcut is push-to-talk.
const HOLD_THRESHOLD: Duration = Duration::from_millis(350);
/// Shorter recordings are accidental taps.
const MIN_RECORDING_MS: i64 = 300;
/// Peak amplitude below which a recording is treated as silence (about -40 dBFS).
const SILENCE_PEAK: i32 = 330;

struct Active {
    id: u64,
    recorder: Recorder,
    settings: Settings,
    started_at_ms: i64,
    frontmost: Option<FrontmostApp>,
    /// Mutes system output while recording, when the setting is on.
    muting: Option<Muting<output_mute::Backend>>,
    watchdog: JoinHandle<()>,
    /// Set for an edit: the selection read started when the edit shortcut went down.
    selection: Option<SelectionTask>,
    /// Where `settings.style` (resolved for the frontmost app at the start) came from.
    style_source: StyleSource,
    /// Spelling hints from the focused field, when context is on.
    context: Option<ContextTask>,
    /// The live transcription stream, when the voice model streams.
    live: Option<LiveSession>,
}

struct Coordinator {
    app: AppHandle,
    active: Option<Active>,
    /// The current recording was started by pressing the hold shortcut.
    hold_started: bool,
    /// When the shared hold/toggle shortcut started the current recording.
    pressed_at: Option<Instant>,
}

pub(crate) fn spawn_coordinator(app: AppHandle, mut control: mpsc::UnboundedReceiver<Control>) {
    tauri::async_runtime::spawn(async move {
        let mut coordinator = Coordinator {
            app,
            active: None,
            hold_started: false,
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
            Control::CaptureEnded(id) => {
                if self.active.as_ref().is_some_and(|active| active.id == id) {
                    tracing::info!(session = id, "capture ended by itself");
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
            // Edit mode toggles: press to record the instruction, press again to apply it.
            (Binding::Edit, true) => {
                if self.active.is_some() {
                    self.stop().await;
                } else {
                    self.start(true).await;
                }
            }
            (Binding::Escape | Binding::Toggle | Binding::Edit, false) => {}
        }
    }

    /// Discards the recording that this press of the shortcut started, if it started one.
    async fn chord(&mut self, binding: Binding) {
        let started_by_press = match binding {
            Binding::Hold => self.hold_started,
            Binding::Both => self.pressed_at.is_some(),
            Binding::Toggle | Binding::Edit | Binding::Escape => false,
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
        let pressed = Instant::now();
        let app = self.app.clone();
        // Read before anything else, while the selection is surely still in place.
        let selection = edit.then(|| capture_selection(&app));
        let state = app.state::<AppState>();
        let id = state.dictation.next_session();

        let mut settings = match state.db() {
            Ok(db) => match db.settings().await {
                Ok(settings) => settings,
                Err(error) => return self.fail_start(id, None, &error.to_string()),
            },
            Err(error) => return self.fail_start(id, None, &error.to_string()),
        };
        // Before anything this recording pastes can reach the watched field.
        learning::prepare(&app, &settings);
        // The formatting model loads while the user speaks instead of after the recording.
        llm::warm_up(&app, &settings);

        let (frontmost, permissions) = tokio::join!(
            tokio::time::timeout(START_PROBE_TIMEOUT, state.engine.frontmost_app()),
            tokio::time::timeout(START_PROBE_TIMEOUT, state.engine.permissions()),
        );
        if let Ok(Ok(permissions)) = permissions {
            permissions::observe(&app, permissions);
            state
                .health
                .update(|facts| facts.permissions = Some(permissions));
        }
        // A dictation that cannot work is refused now, with the reason and its fix, instead of
        // recording into nothing: no microphone, microphone access off, no voice model, engine
        // down.
        if let Some(issue) = health::blocking_issue(&app).await {
            return self.refuse(id, &settings, &issue);
        }
        let frontmost = frontmost.ok().and_then(Result::ok);
        // The style is decided for the app in front at the start and kept for this recording.
        let (style, style_source) = settings.style_for(
            frontmost
                .as_ref()
                .and_then(|front| front.bundle_id.as_deref()),
            frontmost.as_ref().and_then(|front| front.name.as_deref()),
        );
        settings.style = style;
        // An edit's instruction is neither streamed nor primed with the field it edits.
        let context = if edit {
            None
        } else {
            context::capture(&app, &settings, frontmost.as_ref())
        };
        let (live, sink) = if settings.live_transcript && !edit {
            let (live, sink) = live::start(&app, id, &settings);
            (Some(live), Some(sink))
        } else {
            (None, None)
        };

        // Mutes in the background once the start cue played, so the microphone never waits.
        let muting = settings
            .mute_while_recording
            .then(|| Muting::start(output_mute::backend(&app), output_mute::marker_path()));
        let recorder = {
            let microphone = settings.microphone.clone();
            let hooks = CaptureHooks {
                on_level: level_emitter(app.clone()),
                on_chunk: sink,
                on_problem: Some(problem_reporter(app.clone(), id)),
            };
            tauri::async_runtime::spawn_blocking(move || {
                Recorder::start(microphone.as_deref(), hooks, true)
            })
            .await
        };
        let recorder = match recorder {
            Ok(Ok(recorder)) => recorder,
            Ok(Err(problem)) => {
                abandon(&app, live, context).await;
                if let Some(muting) = muting {
                    muting.end().await;
                }
                health::capture_problem(&state, &problem);
                return self.refuse(id, &settings, &problem.issue(Platform::current()));
            }
            Err(error) => {
                abandon(&app, live, context).await;
                if let Some(muting) = muting {
                    muting.end().await;
                }
                let message = format!("Could not start the microphone: {error}");
                return self.fail_start(id, Some(&settings), &message);
            }
        };
        // It opened: whatever kept the last one from opening is over.
        health::capture_finished(&state, None);
        tracing::info!(
            session = id,
            device = recorder.device(),
            elapsed_ms = pressed.elapsed().as_millis(),
            "microphone open"
        );

        state.play(&settings, Cue::Start);
        if let Some(muting) = &muting {
            muting.cue_played();
        }
        let started_at_ms = now_ms();
        state
            .dictation
            .recording_session
            .store(id, Ordering::SeqCst);

        let mut recording = if edit {
            edit_state(DictationPhase::Recording, id)
        } else {
            DictationState::new(DictationPhase::Recording, id)
        };
        recording.started_at = Some(started_at_ms);
        publish(&app, recording);
        if settings.escape_cancels {
            shortcuts::set_escape(&app, true);
        }

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
            muting,
            watchdog,
            selection,
            style_source,
            context,
            live,
        });
    }

    fn fail_start(&self, id: u64, settings: Option<&Settings>, message: &str) {
        tracing::warn!(session = id, message, "dictation could not start");
        if let Some(settings) = settings {
            self.app.state::<AppState>().play(settings, Cue::Error);
        }
        publish_message(&self.app, id, DictationPhase::Error, message);
    }

    /// Refuses a dictation for `issue`: the overlay shows it, and on Linux, which has no
    /// overlay, the window opens on its banner, so the reason is never invisible.
    fn refuse(&self, id: u64, settings: &Settings, issue: &Issue) {
        self.fail_start(id, Some(settings), &issue.sentence());
        if Platform::current() == Platform::Linux {
            windows::show_main(&self.app);
        }
    }

    /// Ends the recording span shared by stop and cancel: watchdog, Esc, recorder, muting.
    async fn finish_recording(&mut self) -> Option<Ended> {
        let Active {
            id,
            recorder,
            settings,
            started_at_ms,
            frontmost,
            muting,
            watchdog,
            selection,
            style_source,
            context,
            live,
        } = self.active.take()?;
        self.hold_started = false;
        self.pressed_at = None;
        watchdog.abort();
        self.app
            .state::<AppState>()
            .dictation
            .recording_session
            .store(0, Ordering::SeqCst);
        shortcuts::set_escape(&self.app, false);
        let recording = tauri::async_runtime::spawn_blocking(move || recorder.stop())
            .await
            .map_err(AppError::audio)
            .and_then(|result| result);
        // After the microphone stopped, so the unmuted sound is not recorded, and before the stop
        // cue, so the cue is audible.
        if let Some(muting) = muting {
            muting.end().await;
        }
        Some(Ended {
            id,
            settings,
            started_at_ms,
            frontmost,
            recording,
            selection,
            style_source,
            context,
            live,
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
                abandon(&self.app, ended.live, ended.context).await;
                state.play(&ended.settings, Cue::Error);
                let message = format!("Recording failed: {error}");
                return publish_message(&self.app, ended.id, DictationPhase::Error, message);
            }
        };
        health::capture_finished(&state, recording.problem.as_ref());
        // What the microphone did wrong explains a short or silent recording better than "Too
        // short"; a recording cut off by an unplugged microphone is still transcribed below.
        let problem = recording
            .problem
            .as_ref()
            .map(|problem| problem.issue(Platform::current()).sentence());
        let unusable = if recording.duration_ms < MIN_RECORDING_MS {
            Some("Too short")
        } else if is_silent(&recording.samples) {
            Some("No speech detected")
        } else {
            None
        };
        if let Some(reason) = unusable {
            abandon(&self.app, ended.live, ended.context).await;
            return match problem {
                Some(message) => {
                    state.play(&ended.settings, Cue::Error);
                    publish_message(&self.app, ended.id, DictationPhase::Error, message);
                }
                None => publish_message(&self.app, ended.id, DictationPhase::Cancelled, reason),
            };
        }
        if let Some(message) = problem {
            tracing::warn!(session = ended.id, %message, "transcribing what was recorded");
        }
        let input = SessionInput {
            id: ended.id,
            recording,
            settings: ended.settings,
            started_at_ms: ended.started_at_ms,
            stopped_at,
            frontmost: ended.frontmost,
            selection: ended.selection,
            style_source: ended.style_source,
            context: ended.context,
            live: ended.live,
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
        abandon(&self.app, ended.live, ended.context).await;
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
    style_source: StyleSource,
    context: Option<ContextTask>,
    live: Option<LiveSession>,
}

/// Ends the live stream and the context read of a recording that will not be transcribed.
async fn abandon(app: &AppHandle, live: Option<LiveSession>, context: Option<ContextTask>) {
    if let Some(context) = context {
        context.abort();
    }
    if let Some(live) = live {
        live.end(app).await;
    }
}

fn is_silent(samples: &[i16]) -> bool {
    samples
        .iter()
        .all(|sample| i32::from(*sample).abs() < SILENCE_PEAK)
}
