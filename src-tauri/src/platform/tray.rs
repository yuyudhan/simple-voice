// FilePath: src-tauri/src/platform/tray.rs
//! Menu bar icon: start/stop dictation, open the window or Settings, updates, quit, and the
//! most important health issue at the top, which opens the window that explains it.

use std::fmt;
use std::sync::Mutex;

use sv_domain::Health;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::features::dictation::Control;
use crate::features::updates;
use crate::platform::windows;
use crate::state::{lock, AppState};

const TRAY_ID: &str = "simple-voice";
const TOGGLE: &str = "tray-toggle";
const OPEN: &str = "tray-open";
const SETTINGS: &str = "tray-settings";
const UPDATES: &str = "tray-updates";
const QUIT: &str = "tray-quit";
const HEALTH: &str = "tray-health";
const TOOLTIP: &str = "Simple Voice";

/// macOS tints the monochrome template to the menu bar; Linux panels show icons as they are, so
/// they get the app icon, which reads on light and dark panels alike.
#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../../icons/tray-template@2x.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../../icons/64x64.png");

const START_LABEL: &str = "Start dictation";
const STOP_LABEL: &str = "Stop dictation";
const CHECK_LABEL: &str = "Check for Updates…";

/// Holds the items whose labels follow app state: dictation, update availability and health.
pub(crate) struct TrayMenu {
    menu: Menu<Wry>,
    toggle: MenuItem<Wry>,
    updates: MenuItem<Wry>,
    health: MenuItem<Wry>,
    health_separator: PredefinedMenuItem<Wry>,
    /// Whether the health item and its separator are in the menu.
    health_shown: Mutex<bool>,
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
        .icon(Image::from_bytes(TRAY_ICON)?)
        .icon_as_template(true)
        .tooltip(TOOLTIP)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;
    tracing::info!("tray icon created");

    let health = MenuItem::with_id(app, HEALTH, "", true, None::<&str>)?;
    let health_separator = PredefinedMenuItem::separator(app)?;
    app.manage(TrayMenu {
        menu,
        toggle,
        updates,
        health,
        health_separator,
        health_shown: Mutex::new(false),
    });
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        TOGGLE => app.state::<AppState>().dictation.send(Control::Toggle),
        OPEN | HEALTH => windows::show_main(app),
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

/// Shows the most severe issue at the top of the menu and in the tooltip; clicking it opens the
/// window, whose banner explains it and offers the fix. No issue removes the item.
pub(crate) fn set_health(app: &AppHandle, health: &Health) {
    let Some(menu) = app.try_state::<TrayMenu>() else {
        return;
    };
    let top = health.issues.first();
    let tooltip = top.map_or_else(
        || TOOLTIP.to_owned(),
        |issue| format!("{TOOLTIP}: {}", issue.title),
    );
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(error) = tray.set_tooltip(Some(tooltip)) {
            tracing::warn!(%error, "could not update the tray tooltip");
        }
    }
    let mut shown = lock(&menu.health_shown);
    let result = match (top, *shown) {
        (Some(issue), was_shown) => {
            let label = if health.issues.len() > 1 {
                format!("⚠ {} (+{} more)…", issue.title, health.issues.len() - 1)
            } else {
                format!("⚠ {}…", issue.title)
            };
            menu.health.set_text(label).and_then(|()| {
                if was_shown {
                    return Ok(());
                }
                menu.menu.insert(&menu.health, 0)?;
                menu.menu.insert(&menu.health_separator, 1)
            })
        }
        (None, true) => menu
            .menu
            .remove(&menu.health)
            .and_then(|()| menu.menu.remove(&menu.health_separator)),
        (None, false) => Ok(()),
    };
    match result {
        Ok(()) => *shown = top.is_some(),
        Err(error) => tracing::warn!(%error, "could not update the tray menu"),
    }
}
