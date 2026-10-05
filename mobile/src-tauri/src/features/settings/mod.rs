// FilePath: mobile/src-tauri/src/features/settings/mod.rs
//! Settings commands. Unlike the desktop app, a change has no system side effects (no
//! shortcuts, Dock or login item), so a patch is simply stored and announced.

use sv_domain::{AppError, AppResult, Settings, SettingsPatch};
use tauri::{AppHandle, State};

use crate::events;
use crate::state::AppState;

#[tauri::command]
pub(crate) async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.db()?.settings().await
}

#[tauri::command]
pub(crate) async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    let settings = state.db()?.update_settings(patch).await?;
    events::emit(&app, events::SETTINGS_CHANGED, settings.clone());
    Ok(settings)
}

/// Stores the key the keyboard and the formatting pass use; `None` or blank removes it.
#[tauri::command]
pub(crate) async fn set_groq_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    key: Option<String>,
) -> AppResult<Settings> {
    let db = state.db()?;
    let key = key
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty());
    db.set_groq_api_key(key).await?;
    let settings = db.settings().await?;
    events::emit(&app, events::SETTINGS_CHANGED, settings.clone());
    Ok(settings)
}

#[tauri::command]
pub(crate) async fn verify_groq_api_key(state: State<'_, AppState>, key: String) -> AppResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::invalid("Enter a Groq API key"));
    }
    sv_cloud::groq_verify_key(state.http()?, key).await
}
