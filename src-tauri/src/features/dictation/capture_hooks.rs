// FilePath: src-tauri/src/features/dictation/capture_hooks.rs
//! What a dictation's capture reports while it runs: the level for the pill, and the
//! microphone's problems for the app's health and the coordinator.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use sv_audio::ProblemSink;
use tauri::{AppHandle, Manager};

use super::Control;
use crate::features::health;
use crate::platform::overlay;
use crate::state::{lock, AppState};

/// ~30 Hz level updates.
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

/// Forwards the recorder's level to the pill at most ~30 times a second.
pub(super) fn level_emitter(app: AppHandle) -> Box<dyn Fn(f32) + Send + 'static> {
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

/// Records what the microphone does wrong during session `id` in the app's health, and stops the
/// dictation when the problem ended the capture, so the user is never left recording nothing.
pub(super) fn problem_reporter(app: AppHandle, id: u64) -> ProblemSink {
    Box::new(move |problem| {
        let state = app.state::<AppState>();
        health::capture_problem(&state, problem);
        if problem.ends_recording() {
            state.dictation.send(Control::CaptureEnded(id));
        }
    })
}
