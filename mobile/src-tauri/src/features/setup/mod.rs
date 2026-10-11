// FilePath: mobile/src-tauri/src/features/setup/mod.rs
//! Keyboard setup from the app: the Settings tab shows each step (keyboard added, Full Access or
//! selected, microphone) and buttons that open the phone's settings for it. The phone answers
//! through the sv-native plugin; its calls block, so each runs on a blocking thread.

use sv_domain::{AppError, AppResult};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_sv_native::{KeyboardSetup, NativeBridge};

#[tauri::command]
pub(crate) async fn get_keyboard_setup(app: AppHandle) -> AppResult<KeyboardSetup> {
    native(app, |bridge| bridge.keyboard_setup()).await
}

#[tauri::command]
pub(crate) async fn open_keyboard_settings(app: AppHandle) -> AppResult<()> {
    native(app, |bridge| bridge.open_keyboard_settings()).await
}

#[tauri::command]
pub(crate) async fn choose_keyboard(app: AppHandle) -> AppResult<()> {
    native(app, |bridge| bridge.choose_keyboard()).await
}

/// Returns the microphone permission after the prompt: `granted`, `denied` or `undetermined`.
#[tauri::command]
pub(crate) async fn request_microphone(app: AppHandle) -> AppResult<String> {
    native(app, |bridge| bridge.request_microphone()).await
}

async fn native<T: Send + 'static>(
    app: AppHandle,
    call: impl FnOnce(&NativeBridge<Wry>) -> Result<T, String> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || call(&app.state::<NativeBridge<Wry>>()))
        .await
        .map_err(AppError::other)?
        .map_err(AppError::other)
}
