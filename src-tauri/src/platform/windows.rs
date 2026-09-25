// FilePath: src-tauri/src/platform/windows.rs
//! The main window and the app menu.

use tauri::menu::{Menu, MenuItem, MenuItemKind};
use tauri::window::Color;
use tauri::{
    AppHandle, LogicalPosition, Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

use crate::events;

pub(crate) const MAIN: &str = "main";
const APP_MENU_SETTINGS: &str = "app-settings";

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
            if let Err(error) = handle.hide() {
                tracing::warn!(%error, "could not hide the main window");
            }
        }
    });
    Ok(())
}

pub(crate) fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN) else {
        tracing::warn!("main window missing");
        return;
    };
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

/// The default macOS menu plus "Settings…" (Cmd+,) in the app menu.
pub(crate) fn install_app_menu(app: &AppHandle) -> tauri::Result<()> {
    let menu = Menu::default(app)?;
    let settings = MenuItem::with_id(
        app,
        APP_MENU_SETTINGS,
        "Settings…",
        true,
        Some("CmdOrCtrl+,"),
    )?;
    if let Some(MenuItemKind::Submenu(app_menu)) = menu.items()?.into_iter().next() {
        // After "About Simple Voice" and its separator, where macOS apps put Settings.
        let position = app_menu.items()?.len().min(2);
        app_menu.insert(&settings, position)?;
    }
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id().as_ref() == APP_MENU_SETTINGS {
            open_settings(app);
        }
    });
    Ok(())
}
