// FilePath: engine-linux/src/linux/desktop.rs
//! The desktop side of dictation on Linux: pasting into the focused app, which app that is, the
//! selected text, and what the permission screens show.
//!
//! Linux has no per-app microphone or accessibility grant, so "accessibility" here means "a
//! tool that can press Ctrl+V in this session": xdotool under X11; under Wayland ydotool with
//! write access to /dev/uinput, or wtype on compositors with the virtual-keyboard protocol
//! (not GNOME). Without one the text stays on the clipboard and the app says so.
//!
//! The paste goes where the dictation started, as on macOS: `frontmost_app` reports the focused
//! window (an X11 window id, or a Sway container id), and `paste` focuses that window again
//! before pressing the keys. Terminals do not paste on Ctrl+V (they pass it to the program
//! inside, where it is quoted-insert in a shell or the TUI's own clipboard reader), so a
//! terminal gets its paste shortcut, Ctrl+Shift+V, which makes the terminal paste once itself.

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

/// The keys that paste into a window: Ctrl+V, or Ctrl+Shift+V in a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteKeys {
    CtrlV,
    CtrlShiftV,
}

/// Window classes (X11 `WM_CLASS`, Wayland `app_id`) of terminals whose paste shortcut is
/// Ctrl+Shift+V. xterm and urxvt have none by default, so they keep Ctrl+V.
const TERMINALS: &[&str] = &[
    "alacritty",
    "blackbox",
    "contour",
    "foot",
    "footclient",
    "ghostty",
    "guake",
    "kitty",
    "konsole",
    "lxterminal",
    "ptyxis",
    "qterminal",
    "rio",
    "sakura",
    "st-256color",
    "terminator",
    "termite",
    "tilix",
    "wezterm",
    "xfce4-terminal",
];

fn paste_keys(class: Option<&str>) -> PasteKeys {
    let Some(class) = class.map(str::to_ascii_lowercase) else {
        return PasteKeys::CtrlV;
    };
    // Reverse-DNS classes (`com.mitchellh.ghostty`, `org.wezfurlong.wezterm`) end in the name.
    let short = class.rsplit('.').next().unwrap_or(&class);
    let terminal = TERMINALS.contains(&short)
        || TERMINALS.contains(&class.as_str())
        || short.ends_with("-terminal")
        || short == "terminal";
    if terminal {
        PasteKeys::CtrlShiftV
    } else {
        PasteKeys::CtrlV
    }
}

/// Focuses `window` (from `frontmost_app`) when given, then presses the paste keys once; the
/// app has put the text on the clipboard. Fails without pressing anything when the window is
/// gone or will not take focus, so the text stays on the clipboard instead of landing elsewhere.
pub(crate) async fn paste(window: Option<&str>) -> Result<(), String> {
    let paster = paster().ok_or(PASTE_UNAVAILABLE)?;
    let target = match window {
        Some(window) => Some(focus(window).await?),
        None => focused_window().await,
    };
    let keys = paste_keys(target.as_ref().and_then(|target| target.class.as_deref()));
    press(paster, keys).await
}

async fn press(paster: Paster, keys: PasteKeys) -> Result<(), String> {
    match (paster, keys) {
        (Paster::Xdotool, PasteKeys::CtrlV) => {
            tools::run_ok("xdotool", &["key", "--clearmodifiers", "ctrl+v"]).await
        }
        (Paster::Xdotool, PasteKeys::CtrlShiftV) => {
            tools::run_ok("xdotool", &["key", "--clearmodifiers", "ctrl+shift+v"]).await
        }
        // ydotool 1.x takes evdev key codes (29 = left Ctrl, 42 = left Shift, 47 = V); 0.1.x,
        // which Ubuntu 22.04 ships, takes key names. A 1.x run that fails pressed nothing.
        (Paster::Ydotool, PasteKeys::CtrlV) => {
            match tools::run_ok("ydotool", &["key", "29:1", "47:1", "47:0", "29:0"]).await {
                Ok(out) => Ok(out),
                Err(_) => tools::run_ok("ydotool", &["key", "ctrl+v"]).await,
            }
        }
        (Paster::Ydotool, PasteKeys::CtrlShiftV) => {
            let codes = ["key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0"];
            match tools::run_ok("ydotool", &codes).await {
                Ok(out) => Ok(out),
                Err(_) => tools::run_ok("ydotool", &["key", "ctrl+shift+v"]).await,
            }
        }
        (Paster::Wtype, PasteKeys::CtrlV) => {
            tools::run_ok("wtype", &["-M", "ctrl", "v", "-m", "ctrl"]).await
        }
        (Paster::Wtype, PasteKeys::CtrlShiftV) => {
            let args = [
                "-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl",
            ];
            tools::run_ok("wtype", &args).await
        }
    }
    .map(drop)
}

/// A focused window: its id (X11 window, or Sway container) and application class.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Window {
    id: String,
    class: Option<String>,
}

const FOCUS_LOST: &str = "the window you dictated into is closed or would not take focus";

/// Makes `id` the focused window again and confirms it is.
async fn focus(id: &str) -> Result<Window, String> {
    if let Some(now) = focused_window().await.filter(|now| now.id == id) {
        return Ok(now);
    }
    match session() {
        Session::X11 if tools::installed("xdotool") => {
            tools::run_ok("xdotool", &["windowactivate", "--sync", id])
                .await
                .map_err(|_| FOCUS_LOST.to_owned())?;
        }
        Session::Wayland if is_sway() => {
            let criteria = format!("[con_id={id}]");
            tools::run_ok("swaymsg", &[criteria.as_str(), "focus"])
                .await
                .map_err(|_| FOCUS_LOST.to_owned())?;
        }
        // No way to focus a window here: paste where the focus is.
        _ => {
            return Ok(Window {
                id: id.to_owned(),
                class: None,
            })
        }
    }
    focused_window()
        .await
        .filter(|now| now.id == id)
        .ok_or_else(|| FOCUS_LOST.to_owned())
}

fn is_sway() -> bool {
    std::env::var_os("SWAYSOCK").is_some() && tools::installed("swaymsg")
}

/// The focused window under X11 (xdotool) or Sway (swaymsg). Other Wayland compositors do not
/// tell clients which window has focus.
async fn focused_window() -> Option<Window> {
    match session() {
        Session::X11 if tools::installed("xdotool") => {
            let id = tools::run_ok("xdotool", &["getactivewindow"]).await.ok()?;
            let id = id.trim().to_owned();
            if id.is_empty() {
                return None;
            }
            let class = x11_class(&id).await;
            Some(Window { id, class })
        }
        Session::Wayland if is_sway() => {
            let tree = tools::run_ok("swaymsg", &["-t", "get_tree", "--raw"])
                .await
                .ok()?;
            sway_focused(&serde_json::from_str(&tree).ok()?)
        }
        _ => None,
    }
}

/// The window's `WM_CLASS` class. xprop is everywhere xdotool is; xdotool before 3.2021 (Ubuntu
/// 22.04) has no `getwindowclassname`.
async fn x11_class(id: &str) -> Option<String> {
    if tools::installed("xprop") {
        let out = tools::run_ok("xprop", &["-id", id, "WM_CLASS"])
            .await
            .ok()?;
        return wm_class(&out);
    }
    let class = tools::run_ok("xdotool", &["getwindowclassname", id])
        .await
        .ok()?;
    Some(class.trim().to_owned()).filter(|class| !class.is_empty())
}

/// `WM_CLASS(STRING) = "ghostty", "com.mitchellh.ghostty"` → the class, the last string.
fn wm_class(xprop: &str) -> Option<String> {
    let (_, values) = xprop.split_once('=')?;
    let class = values.rsplit(',').next()?.trim().trim_matches('"');
    Some(class.to_owned()).filter(|class| !class.is_empty())
}

/// The focused view in a `swaymsg -t get_tree` tree.
fn sway_focused(node: &Value) -> Option<Window> {
    if node["focused"].as_bool() == Some(true) && node["type"] != "workspace" {
        let id = node["id"].as_u64()?.to_string();
        let class = node["app_id"]
            .as_str()
            .or_else(|| node["window_properties"]["class"].as_str())
            .map(str::to_owned);
        return Some(Window { id, class });
    }
    ["nodes", "floating_nodes"]
        .iter()
        .filter_map(|key| node[*key].as_array())
        .flatten()
        .find_map(sway_focused)
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FrontmostApp {
    name: Option<String>,
    bundle_id: Option<String>,
    /// The focused window, for `paste` to focus again.
    window: Option<String>,
}

/// The focused window and its application class (`code`, `firefox`) under X11 and Sway. Other
/// Wayland compositors do not tell clients which window has focus, so all stay empty there.
pub(crate) async fn frontmost_app() -> FrontmostApp {
    let Some(window) = focused_window().await else {
        return FrontmostApp::default();
    };
    FrontmostApp {
        name: window.class.clone(),
        bundle_id: window.class.map(|class| class.to_ascii_lowercase()),
        window: Some(window.id),
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

    #[test]
    fn terminals_get_their_own_paste_shortcut() {
        for class in ["com.mitchellh.ghostty", "kitty", "Alacritty", "foot"] {
            assert_eq!(paste_keys(Some(class)), PasteKeys::CtrlShiftV, "{class}");
        }
        assert_eq!(
            paste_keys(Some("org.wezfurlong.wezterm")),
            PasteKeys::CtrlShiftV
        );
        assert_eq!(paste_keys(Some("Gnome-terminal")), PasteKeys::CtrlShiftV);
        for class in ["firefox", "Code", "Google-chrome", "XTerm", "Zenity"] {
            assert_eq!(paste_keys(Some(class)), PasteKeys::CtrlV, "{class}");
        }
        assert_eq!(paste_keys(None), PasteKeys::CtrlV);
    }

    #[test]
    fn reads_the_class_from_xprop() {
        let out = "WM_CLASS(STRING) = \"ghostty\", \"com.mitchellh.ghostty\"\n";
        assert_eq!(wm_class(out).as_deref(), Some("com.mitchellh.ghostty"));
        assert_eq!(wm_class("WM_CLASS:  not found.\n"), None);
    }

    #[test]
    fn finds_the_focused_sway_view() {
        let tree = json!({
            "id": 1, "type": "root", "focused": false,
            "nodes": [{
                "id": 4, "type": "workspace", "focused": false,
                "nodes": [{ "id": 7, "type": "con", "focused": false, "app_id": "firefox" }],
                "floating_nodes": [{
                    "id": 9, "type": "floating_con", "focused": true, "app_id": null,
                    "window_properties": { "class": "XTerm" }
                }]
            }]
        });
        assert_eq!(
            sway_focused(&tree),
            Some(Window {
                id: "9".to_owned(),
                class: Some("XTerm".to_owned())
            })
        );
        let empty = json!({ "id": 1, "type": "workspace", "focused": true, "nodes": [] });
        assert_eq!(sway_focused(&empty), None);
    }
}
