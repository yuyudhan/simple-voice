// FilePath: src-tauri/src/platform/dock.rs
//! Dock visibility.

use tauri::{ActivationPolicy, AppHandle};

pub(crate) fn apply_dock(app: &AppHandle, show: bool) {
    let policy = if show {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    };
    if let Err(error) = app.set_activation_policy(policy) {
        tracing::warn!(%error, show, "could not change Dock visibility");
    }
}
