// FilePath: src-tauri/src/features/dictation/mod.rs
//! Dictation: the recording coordinator, the transcription → formatting → post-processing
//! pipeline, and ordered delivery into the frontmost app.

mod coordinator;
mod delivery;
mod pipeline;

use std::sync::atomic::{AtomicU64, Ordering};

use sv_domain::{AppResult, DictationPhase, DictationState};
use tauri::{AppHandle, Manager, State};
use tokio::sync::mpsc;

pub(crate) use self::coordinator::spawn_coordinator;
pub(crate) use self::delivery::copy_to_clipboard;
pub(crate) use self::pipeline::{retry, test_post_processing, PostProcessingTest};
use crate::events;
use crate::platform::shortcuts::Binding;
use crate::platform::{overlay, tray};
use crate::state::AppState;

/// Everything that can happen to the recorder. All of it is processed in order by one task, so
/// a quick press-and-release can never be handled release-first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Start,
    Stop,
    Cancel,
    Toggle,
    /// A global shortcut went down (`true`) or up (`false`).
    Shortcut(Binding, bool),
    /// The held shortcut key turned out to be part of another key combination (Fn+Arrow).
    ShortcutChord(Binding),
    /// The maximum recording length of this session elapsed.
    Watchdog(u64),
}

/// Shared view of the coordinator, readable from any thread.
#[derive(Debug)]
pub(crate) struct DictationHandle {
    control: mpsc::UnboundedSender<Control>,
    /// Id of the most recently started session.
    latest_session: AtomicU64,
    /// Id of the session currently recording; 0 when none.
    recording_session: AtomicU64,
    /// Id of the newest session whose text was pasted; older results arriving later are dropped.
    delivered: AtomicU64,
}

impl DictationHandle {
    pub(crate) fn new() -> (Self, mpsc::UnboundedReceiver<Control>) {
        let (control, receiver) = mpsc::unbounded_channel();
        let handle = Self {
            control,
            latest_session: AtomicU64::new(0),
            recording_session: AtomicU64::new(0),
            delivered: AtomicU64::new(0),
        };
        (handle, receiver)
    }

    pub(crate) fn send(&self, control: Control) {
        if self.control.send(control).is_err() {
            tracing::error!(?control, "dictation coordinator is not running");
        }
    }

    pub(crate) fn is_recording(&self) -> bool {
        self.recording_session.load(Ordering::SeqCst) != 0
    }

    fn next_session(&self) -> u64 {
        self.latest_session.fetch_add(1, Ordering::SeqCst) + 1
    }
}

/// Shows a session's state in the overlay, tray and UI. States of a session older than the
/// newest one would overwrite what the user is looking at, so they are suppressed; an older
/// session's error still surfaces once nothing is recording (a dictation is never lost silently,
/// and its history row carries the details).
pub(crate) fn publish(app: &AppHandle, state: DictationState) {
    let handle = &app.state::<AppState>().dictation;
    let latest = handle.latest_session.load(Ordering::SeqCst);
    let stale = state.session_id < latest;
    if stale && (state.phase != DictationPhase::Error || handle.is_recording()) {
        tracing::debug!(session = state.session_id, phase = ?state.phase, "stale state skipped");
        return;
    }
    tray::set_recording(app, state.phase == DictationPhase::Recording);
    overlay::follow(app, &state);
    events::emit(app, events::DICTATION_STATE, state);
}

pub(crate) fn publish_phase(app: &AppHandle, session_id: u64, phase: DictationPhase) {
    publish(app, DictationState::new(phase, session_id));
}

pub(crate) fn publish_message(
    app: &AppHandle,
    session_id: u64,
    phase: DictationPhase,
    message: impl Into<String>,
) {
    let mut state = DictationState::new(phase, session_id);
    state.message = Some(message.into());
    publish(app, state);
}

#[tauri::command]
pub(crate) fn start_dictation(state: State<'_, AppState>) -> AppResult<()> {
    state.dictation.send(Control::Start);
    Ok(())
}

#[tauri::command]
pub(crate) fn stop_dictation(state: State<'_, AppState>) -> AppResult<()> {
    state.dictation.send(Control::Stop);
    Ok(())
}

#[tauri::command]
pub(crate) fn cancel_dictation(state: State<'_, AppState>) -> AppResult<()> {
    state.dictation.send(Control::Cancel);
    Ok(())
}

#[tauri::command]
pub(crate) fn toggle_dictation(state: State<'_, AppState>) -> AppResult<()> {
    state.dictation.send(Control::Toggle);
    Ok(())
}
