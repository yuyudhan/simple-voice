// FilePath: mobile/src-tauri/src/events.rs
//! Event names shared with the desktop app (docs/internal/architecture.md § 4), so the reused
//! screens subscribe to the same names.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub(crate) const HISTORY_CHANGED: &str = "history-changed";
pub(crate) const SETTINGS_CHANGED: &str = "settings-changed";

/// Emits to every webview. A failed emit only means nothing is listening, so it is logged.
pub(crate) fn emit<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) {
    if let Err(error) = app.emit(event, payload) {
        tracing::warn!(%error, event, "emit failed");
    }
}
