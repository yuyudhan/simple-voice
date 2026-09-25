// FilePath: src-tauri/src/events.rs
//! Event names of docs/architecture.md § 4 and the one place that emits them.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub(crate) const DICTATION_STATE: &str = "dictation-state";
pub(crate) const DICTATION_LEVEL: &str = "dictation-level";
pub(crate) const HISTORY_CHANGED: &str = "history-changed";
pub(crate) const SETTINGS_CHANGED: &str = "settings-changed";
pub(crate) const MODEL_PROGRESS: &str = "model-progress";
pub(crate) const PERMISSIONS_CHANGED: &str = "permissions-changed";
pub(crate) const NAVIGATE: &str = "navigate";

#[derive(Debug, Clone, Copy, Serialize)]
pub(crate) struct Level {
    pub(crate) level: f32,
}

/// Emits to every window. A failed emit only means no webview is listening, so it is logged.
pub(crate) fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(error) = app.emit(event, payload) {
        tracing::warn!(%error, event, "emit failed");
    }
}

pub(crate) fn emit_to<S: Serialize + Clone>(app: &AppHandle, label: &str, event: &str, payload: S) {
    if let Err(error) = app.emit_to(label, event, payload) {
        tracing::warn!(%error, event, label, "emit failed");
    }
}

pub(crate) fn history_changed(app: &AppHandle) {
    emit(app, HISTORY_CHANGED, ());
}
