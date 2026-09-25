// FilePath: src-tauri/src/platform/dock.rs
//! Dock visibility and launch at login.

use tauri::{ActivationPolicy, AppHandle};
use tauri_plugin_autostart::ManagerExt;

/// Passed by the login item so a launch at login starts quietly in the menu bar.
pub(crate) const BACKGROUND_ARG: &str = "--background";

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

/// Makes the login item match the setting; the check avoids rewriting the LaunchAgent plist on
/// every launch.
pub(crate) fn sync_autostart(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();
    let current = manager.is_enabled().unwrap_or(!enabled);
    if current == enabled {
        return;
    }
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(error) = result {
        tracing::warn!(%error, enabled, "could not update launch at login");
    }
}
