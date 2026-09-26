// FilePath: src-tauri/src/platform/overlay.rs
//! The floating pill shown while dictating. The Swift helper draws it (docs/internal/engine.md):
//! only AppKit window flags let a window float over full-screen apps, and the helper, being a
//! separate accessory process, can show it without ever activating Simple Voice. This module
//! decides what the pill shows and when, and sends it as `overlay_*` notifications.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use sv_domain::{AppResult, DictationPhase, DictationState};
use tauri::{AppHandle, Manager};

use crate::state::{lock, AppState};

const HIDE_DELAY: Duration = Duration::from_millis(1200);
/// A finished dictation shows the start of its text long enough to read it.
const DONE_HIDE_DELAY: Duration = Duration::from_secs(4);

#[derive(Debug, Default)]
pub(crate) struct OverlayState {
    always: AtomicBool,
    /// Whether the pill is meant to be on screen.
    shown: AtomicBool,
    /// Bumped on every phase change so a pending hide is skipped once a newer phase arrives.
    generation: AtomicU64,
    /// The last state sent, replayed to a restarted helper.
    last: Mutex<Option<DictationState>>,
}

/// Keeps the pill visible at all times when the user asked for it.
pub(crate) fn set_always(app: &AppHandle, always: bool) {
    app.state::<OverlayState>()
        .always
        .store(always, Ordering::SeqCst);
    if always {
        set_visible(app, true);
    } else if !app.state::<AppState>().dictation.is_recording() {
        set_visible(app, false);
    }
}

/// Follows the published dictation state: visible while a session is active, hidden shortly
/// after it ends.
pub(crate) fn follow(app: &AppHandle, state: &DictationState) {
    let overlay = app.state::<OverlayState>();
    *lock(&overlay.last) = Some(state.clone());
    send("state", app.state::<AppState>().engine.overlay_state(state));
    let generation = overlay.generation.fetch_add(1, Ordering::SeqCst) + 1;
    match state.phase {
        DictationPhase::Idle => {
            if !overlay.always.load(Ordering::SeqCst) {
                set_visible(app, false);
            }
        }
        DictationPhase::Recording | DictationPhase::Transcribing | DictationPhase::Formatting => {
            set_visible(app, true);
        }
        DictationPhase::Done | DictationPhase::Error | DictationPhase::Cancelled => {
            set_visible(app, true);
            let delay = if state.phase == DictationPhase::Done {
                DONE_HIDE_DELAY
            } else {
                HIDE_DELAY
            };
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(delay).await;
                let overlay = app.state::<OverlayState>();
                let unchanged = overlay.generation.load(Ordering::SeqCst) == generation;
                if unchanged && !overlay.always.load(Ordering::SeqCst) {
                    set_visible(&app, false);
                }
            });
        }
    }
}

/// Forwards the recorder's level to the pill's meter.
pub(crate) fn level(app: &AppHandle, level: f32) {
    send("level", app.state::<AppState>().engine.overlay_level(level));
}

/// Replays the pill to a freshly started helper, which starts out hidden and idle.
pub(crate) fn resync(app: &AppHandle) {
    let overlay = app.state::<OverlayState>();
    let engine = &app.state::<AppState>().engine;
    if let Some(state) = lock(&overlay.last).clone() {
        send("state", engine.overlay_state(&state));
    }
    send(
        "visibility",
        engine.overlay_visible(overlay.shown.load(Ordering::SeqCst)),
    );
}

fn set_visible(app: &AppHandle, visible: bool) {
    app.state::<OverlayState>()
        .shown
        .store(visible, Ordering::SeqCst);
    send(
        "visibility",
        app.state::<AppState>().engine.overlay_visible(visible),
    );
}

/// The pill is best effort: while the helper is down (it restarts on its own and gets a
/// `resync`), dictation carries on without it.
fn send(what: &str, result: AppResult<()>) {
    if let Err(error) = result {
        tracing::debug!(%error, what, "overlay update not delivered");
    }
}
