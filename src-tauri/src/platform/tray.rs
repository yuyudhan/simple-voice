// FilePath: src-tauri/src/platform/tray.rs
//! Menu bar icon: start/stop dictation, open the window or Settings, updates, quit.

use std::fmt;

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::features::dictation::Control;
use crate::features::updates;
use crate::platform::windows;
use crate::state::AppState;

const TRAY_ID: &str = "simple-voice";
const TOGGLE: &str = "tray-toggle";
const OPEN: &str = "tray-open";
const SETTINGS: &str = "tray-settings";
const UPDATES: &str = "tray-updates";
const QUIT: &str = "tray-quit";

const START_LABEL: &str = "Start dictation";
const STOP_LABEL: &str = "Stop dictation";
const CHECK_LABEL: &str = "Check for Updates…";

/// Holds the items whose labels follow app state: dictation and update availability.
pub(crate) struct TrayMenu {
    toggle: MenuItem<Wry>,
    updates: MenuItem<Wry>,
}

impl fmt::Debug for TrayMenu {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrayMenu").finish_non_exhaustive()
    }
}

pub(crate) fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, TOGGLE, START_LABEL, true, None::<&str>)?;
    let open = MenuItem::with_id(app, OPEN, "Open Simple Voice", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, SETTINGS, "Settings…", true, None::<&str>)?;
    let updates = MenuItem::with_id(app, UPDATES, CHECK_LABEL, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Simple Voice", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &settings,
            &updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(include_bytes!(
            "../../icons/tray-template@2x.png"
        ))?)
        .icon_as_template(true)
        .tooltip("Simple Voice")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;

    app.manage(TrayMenu { toggle, updates });
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        TOGGLE => app.state::<AppState>().dictation.send(Control::Toggle),
        OPEN => windows::show_main(app),
        SETTINGS => windows::open_settings(app),
        UPDATES => updates::open(app),
        QUIT => app.exit(0),
        _ => {}
    }
}

pub(crate) fn set_recording(app: &AppHandle, recording: bool) {
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let label = if recording { STOP_LABEL } else { START_LABEL };
    if let Err(error) = menu.toggle.set_text(label) {
        tracing::warn!(%error, "could not update the tray menu");
    }
}

/// `Some(version)` while a newer release is out; `None` restores "Check for Updates…".
pub(crate) fn set_update(app: &AppHandle, available: Option<&str>) {
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let label = available.map_or_else(
        || CHECK_LABEL.to_owned(),
        |version| format!("Update Available: {version}…"),
    );
    if let Err(error) = menu.updates.set_text(label) {
        tracing::warn!(%error, "could not update the tray menu");
    }
}
