// FilePath: src-tauri/src/platform/dock.rs
//! Dock visibility.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{ActivationPolicy, AppHandle, Manager};

use crate::platform::windows::MAIN;

/// The "Show app in Dock" setting. The icon shows only while the main window is open too, as
/// Docker's does: a Dock icon offers "Quit", which would stop the shortcuts, so with the window
/// closed the app lives in the menu bar alone.
#[derive(Debug, Default)]
pub(crate) struct DockSetting(AtomicBool);

/// Applies the setting at launch and when it changes.
pub(crate) fn apply_dock(app: &AppHandle, show: bool) {
    app.state::<DockSetting>().0.store(show, Ordering::Relaxed);
    let window_open = app
        .get_webview_window(MAIN)
        .is_some_and(|window| window.is_visible().unwrap_or(false));
    set_icon(app, show && window_open);
}

/// Called when the main window is shown or hidden.
pub(crate) fn follow_window(app: &AppHandle, window_open: bool) {
    let show = app.state::<DockSetting>().0.load(Ordering::Relaxed);
    set_icon(app, show && window_open);
}

fn set_icon(app: &AppHandle, show: bool) {
    let policy = if show {
        ActivationPolicy::Regular
    } else {
        ActivationPolicy::Accessory
    };
    if let Err(error) = app.set_activation_policy(policy) {
        tracing::warn!(%error, show, "could not change Dock visibility");
    }
}
