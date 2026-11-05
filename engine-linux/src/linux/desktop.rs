// FilePath: engine-linux/src/linux/desktop.rs
//! The desktop side of dictation on Linux: pasting into the focused app, which app that is, the
//! selected text, and what the permission screens show.
//!
//! Linux has no per-app microphone or accessibility grant, so "accessibility" here means "a
//! tool that can press Ctrl+V in this session": xdotool under X11; under Wayland ydotool with
//! write access to /dev/uinput, or wtype on compositors with the virtual-keyboard protocol
//! (not GNOME). Without one the text stays on the clipboard and the app says so.

use serde::Serialize;
use serde_json::{json, Value};
use sv_domain::{PermissionKind, PermissionStatus, Permissions};

use super::tools;

/// Contains the marker `sv-engine` turns into a permission error, so the app shows its
/// "copied, not pasted" notice.
const PASTE_UNAVAILABLE: &str = "accessibility permission missing: no tool can press Ctrl+V in \
    this Wayland session; install ydotool and give your user write access to /dev/uinput \
    (docs/linux.md)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Session {
    X11,
    Wayland,
}

/// The session the user's apps run in, which decides how keys reach them.
pub(crate) fn session() -> Session {
    session_from(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
    )
}

fn session_from(session_type: Option<&str>, wayland_display: Option<&str>) -> Session {
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("wayland") => Session::Wayland,
        Some("x11") => Session::X11,
        _ if wayland_display.is_some_and(|display| !display.is_empty()) => Session::Wayland,
        _ => Session::X11,
    }
}

fn is_gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .is_ok_and(|desktop| desktop.to_ascii_uppercase().contains("GNOME"))
}

fn uinput_writable() -> bool {
    std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/uinput")
        .is_ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Paster {
    Xdotool,
    Ydotool,
    Wtype,
}

fn paster() -> Option<Paster> {
    match session() {
        Session::X11 => tools::installed("xdotool").then_some(Paster::Xdotool),
        Session::Wayland if tools::installed("ydotool") && uinput_writable() => {
            Some(Paster::Ydotool)
        }
        Session::Wayland if tools::installed("wtype") && !is_gnome() => Some(Paster::Wtype),
        Session::Wayland => None,
    }
}

/// Presses Ctrl+V in the focused app; the app has put the text on the clipboard.
pub(crate) async fn paste() -> Result<(), String> {
    match paster().ok_or(PASTE_UNAVAILABLE)? {
        Paster::Xdotool => tools::run_ok("xdotool", &["key", "--clearmodifiers", "ctrl+v"])
            .await
            .map(drop),
        // ydotool 1.x takes evdev key codes (29 = left Ctrl, 47 = V); 0.1.x, which Ubuntu 22.04
        // ships, takes key names.
        Paster::Ydotool => {
            match tools::run_ok("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]).await {
                Ok(_) => Ok(()),
                Err(_) => tools::run_ok("ydotool", &["key", "ctrl+v"]).await.map(drop),
            }
        }
        Paster::Wtype => tools::run_ok("wtype", &["-M", "ctrl", "v", "-m", "ctrl"])
            .await
            .map(drop),
    }
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrontmostApp {
    name: Option<String>,
    bundle_id: Option<String>,
}

/// The focused window's application class under X11 (`code`, `firefox`). Wayland does not tell
/// other clients which window has focus, so both stay empty there.
pub(crate) async fn frontmost_app() -> FrontmostApp {
    if session() != Session::X11 || !tools::installed("xdotool") {
        return FrontmostApp::default();
    }
    match tools::run_ok("xdotool", &["getactivewindow", "getwindowclassname"]).await {
        Ok(class) if !class.trim().is_empty() => FrontmostApp {
            name: Some(class.trim().to_owned()),
            bundle_id: Some(class.trim().to_ascii_lowercase()),
        },
        _ => FrontmostApp::default(),
    }
}

/// The primary selection: on Linux whatever the user last selected, which is what edit mode
/// rewrites. `None` when it is empty or no tool can read it.
pub(crate) async fn selected_text() -> Value {
    let read = match session() {
        Session::X11 if tools::installed("xclip") => {
            tools::run_ok("xclip", &["-o", "-selection", "primary"]).await
        }
        Session::Wayland if tools::installed("wl-paste") => {
            tools::run_ok("wl-paste", &["--primary", "--no-newline"]).await
        }
        _ => Err(String::new()),
    };
    let text = read.ok().filter(|text| !text.trim().is_empty());
    json!({ "text": text })
}

/// The Linux helper does not read text fields, so context hints only see the app.
pub(crate) async fn focus_context() -> Value {
    let app = frontmost_app().await;
    json!({
        "appName": app.name,
        "bundleId": app.bundle_id,
        "reason": "no_focused_text",
    })
}

pub(crate) fn permissions() -> Permissions {
    Permissions {
        // No desktop gates microphone access per app outside Flatpak; a missing or muted device
        // shows up as a recording error instead.
        microphone: PermissionStatus::Granted,
        accessibility: if paster().is_some() {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        },
        // Apple Speech and system audio capture belong to macOS.
        speech: PermissionStatus::Restricted,
        system_audio: PermissionStatus::Restricted,
    }
}

/// Opens the desktop's sound settings for the microphone; nothing else has a settings page.
pub(crate) async fn open_settings(kind: PermissionKind) -> Result<(), String> {
    match kind {
        PermissionKind::Microphone if tools::installed("gnome-control-center") => {
            let mut child = tokio::process::Command::new("gnome-control-center")
                .arg("sound")
                .spawn()
                .map_err(|error| format!("could not open the sound settings: {error}"))?;
            tokio::spawn(async move { child.wait().await });
            Ok(())
        }
        PermissionKind::Microphone => {
            Err("Choose the microphone in your desktop's sound settings".to_owned())
        }
        PermissionKind::Accessibility => Err(PASTE_UNAVAILABLE.to_owned()),
        PermissionKind::Speech | PermissionKind::SystemAudio => {
            Err("This permission belongs to macOS features that Linux does not have".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_wayland_from_x11() {
        assert_eq!(session_from(Some("wayland"), None), Session::Wayland);
        assert_eq!(session_from(Some("x11"), Some("wayland-0")), Session::X11);
        assert_eq!(
            session_from(Some("tty"), Some("wayland-0")),
            Session::Wayland
        );
        assert_eq!(session_from(None, Some("")), Session::X11);
        assert_eq!(session_from(None, None), Session::X11);
    }

    #[test]
    fn paste_failure_reads_as_missing_access() {
        assert!(PASTE_UNAVAILABLE.starts_with("accessibility permission missing"));
    }
}
