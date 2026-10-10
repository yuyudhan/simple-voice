// FilePath: src-tauri/src/platform/overlay.rs
//! The floating pill shown while dictating. The Swift helper draws it (docs/internal/engine.md):
//! only AppKit window flags let a window float over full-screen apps, and the helper, being a
//! separate accessory process, can show it without ever activating Simple Voice. This module
//! decides what the pill shows and when, and sends it as `overlay_*` notifications.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use sv_domain::{AppResult, DictationPhase, DictationState};
use tauri::{AppHandle, Manager};

use crate::state::{lock, AppState};

const HIDE_DELAY: Duration = Duration::from_millis(1200);
/// The helper keeps the finished text this much longer than the app keeps the pill up, so the
/// text never switches to idle before the 0.15 s fade-out ends.
const DONE_HOLD_MARGIN_MS: u64 = 150;

#[derive(Debug)]
pub(crate) struct OverlayState {
    always: AtomicBool,
    /// How long a finished dictation shows the start of its text (`pastedTextSeconds`).
    pasted_text_seconds: AtomicU32,
    /// Whether the pill is meant to be on screen.
    shown: AtomicBool,
    /// Bumped on every phase change so a pending hide is skipped once a newer phase arrives.
    generation: AtomicU64,
    /// The last state sent, replayed to a restarted helper.
    last: Mutex<Option<DictationState>>,
}

impl OverlayState {
    pub(crate) fn new(pasted_text_seconds: u32) -> Self {
        Self {
            always: AtomicBool::new(false),
            pasted_text_seconds: AtomicU32::new(pasted_text_seconds),
            shown: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            last: Mutex::new(None),
        }
    }

    fn done_delay(&self) -> Duration {
        Duration::from_secs(u64::from(self.pasted_text_seconds.load(Ordering::SeqCst)))
    }

    fn done_hold_ms(&self) -> u64 {
        u64::from(self.pasted_text_seconds.load(Ordering::SeqCst)) * 1000 + DONE_HOLD_MARGIN_MS
    }
}

/// Applies a new `pastedTextSeconds` to the next finished dictation.
pub(crate) fn set_pasted_text_seconds(app: &AppHandle, seconds: u32) {
    app.state::<OverlayState>()
        .pasted_text_seconds
        .store(seconds, Ordering::SeqCst);
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
    send(
        "state",
        app.state::<AppState>()
            .engine
            .overlay_state(state, overlay.done_hold_ms()),
    );
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
                overlay.done_delay()
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
        send(
            "state",
            engine.overlay_state(&state, overlay.done_hold_ms()),
        );
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
