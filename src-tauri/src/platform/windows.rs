// FilePath: src-tauri/src/platform/windows.rs
//! The main window and the app menu.

use tauri::menu::{Menu, MenuItem, MenuItemKind};
use tauri::window::Color;
use tauri::{
    AppHandle, LogicalPosition, Manager, TitleBarStyle, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::events;
use crate::features::updates;
use crate::platform::dock;

pub(crate) const MAIN: &str = "main";
const APP_MENU_SETTINGS: &str = "app-settings";
const APP_MENU_UPDATES: &str = "app-updates";
const APP_MENU_CLOSE: &str = "app-close";
const APP_MENU_QUIT: &str = "app-quit";

/// Matches the UI background so the window never flashes white while the webview loads.
const BACKGROUND: Color = Color(0xFB, 0xF9, 0xF6, 0xFF);

pub(crate) fn create_main(app: &AppHandle, visible: bool) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::default())
        .title("Simple Voice")
        .inner_size(1080.0, 720.0)
        .min_inner_size(860.0, 600.0)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(LogicalPosition::new(18.0, 22.0))
        .background_color(BACKGROUND)
        .visible(visible)
        .build()?;

    // Closing only hides: the app keeps running in the menu bar so the shortcuts keep working.
    let handle = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide_main(&handle);
        }
    });
    Ok(())
}

fn hide_main(window: &WebviewWindow) {
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "could not hide the main window");
    }
    dock::follow_window(window.app_handle(), false);
}

pub(crate) fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("main window missing");
        return;
    };
    dock::follow_window(app, true);
    // An Accessory (no Dock icon) app must be activated explicitly or the window opens behind.
    if let Err(error) = app.show() {
        tracing::debug!(%error, "could not activate the app");
    }
    let shown = window
        .unminimize()
        .and_then(|()| window.show())
        .and_then(|()| window.set_focus());
    if let Err(error) = shown {
        tracing::warn!(%error, "could not show the main window");
    }
}

/// Opens the main window on the Settings dialog.
pub(crate) fn open_settings(app: &AppHandle) {
    show_main(app);
    events::emit_to(app, MAIN, events::NAVIGATE, "settings");
}

/// The default macOS menu plus "Check for Updates…" and "Settings…" (Cmd+,) in the app menu, and
/// Cmd+Q bound to "Close to Menu Bar" rather than quitting.
pub(crate) fn install_app_menu(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    let settings = MenuItem::with_id(
        app,
        APP_MENU_SETTINGS,
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let updates = MenuItem::with_id(
        app,
        APP_MENU_UPDATES,
        "Check for Updates…",
        true,
        None::<&str>,
    )?;
    let close = MenuItem::with_id(
        app,
        APP_MENU_CLOSE,
        "Close to Menu Bar",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let quit = MenuItem::with_id(app, APP_MENU_QUIT, "Quit Simple Voice", true, None::<&str>)?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        // After "About Simple Voice" and its separator, where macOS apps put Settings.
        let position = app_menu.items()?.len().min(2);
        app_menu.insert(&settings, position)?;
        // Directly under "About Simple Voice", where macOS apps put it.
        app_menu.insert(&updates, position.min(1))?;
        // A reflexive Cmd+Q would stop the dictation shortcuts, so it closes the window like the
        // red button; quitting stays one click away here and in the tray. Only the menu's Quit is
        // replaced: the Dock's Quit, logout and the update installer still terminate the app.
        if let Some(MenuItemKind::Predefined(default_quit)) = app_menu.items()?.last() {
            app_menu.remove(default_quit)?;
        }
        app_menu.append_items(&[&close, &quit])?;
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| match event.id().as_ref() {
        APP_MENU_SETTINGS => open_settings(app),
        APP_MENU_UPDATES => updates::open(app),
        APP_MENU_CLOSE => {
            if let Some(window) = app.get_webview_window(MAIN) {
                hide_main(&window);
            }
        }
        APP_MENU_QUIT => app.exit(0),
        _ => {}
    });
    Ok(())
}
