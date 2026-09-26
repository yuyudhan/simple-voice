// FilePath: src-tauri/src/features/settings/mod.rs
//! Settings commands and the side effects of changing a setting (shortcuts, Dock, login item,
//! overlay, model preload).

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use sv_audio::{Cue, Microphone};
use sv_domain::models::LOCAL_TRANSCRIPTION_MODELS;
use sv_domain::{AppError, AppResult, Settings, SettingsPatch, SoundTheme};
use sv_engine::LoginItemStatus;
use tauri::{AppHandle, Manager, State};

use crate::events;
use crate::features::dictation::{self, PostProcessingTest};
use crate::features::models;
use crate::platform::shortcuts::{self, Accelerators};
use crate::platform::{dock, login_item, overlay};
use crate::state::{lock, AppState};

/// Gap between the start and stop cue in a preview.
const PREVIEW_GAP: Duration = Duration::from_millis(700);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppInfo {
    version: String,
    data_dir: String,
    database_path: String,
    models_dir: String,
    engine_version: Option<String>,
}

#[tauri::command]
pub(crate) async fn get_settings(state: State<'_, AppState>) -> AppResult<Settings> {
    state.db()?.settings().await
}

/// Saves a partial update. The login item and shortcut changes are applied first so a change
/// macOS refuses (or an accelerator another app owns) is rejected before anything is stored.
#[tauri::command]
pub(crate) async fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> AppResult<Settings> {
    let db = state.db()?;
    let _login_guard = match patch.launch_at_login {
        Some(_) => Some(state.login_item.lock().await),
        None => None,
    };
    let old = db.settings().await?;
    let login = patch
        .launch_at_login
        .filter(|enabled| *enabled != old.launch_at_login);
    if let Some(enabled) = login {
        login_item::set(&app, enabled).await?;
    }
    let hold = patch
        .hold_shortcut
        .clone()
        .unwrap_or_else(|| old.hold_shortcut.clone());
    let toggle = patch
        .toggle_shortcut
        .clone()
        .unwrap_or_else(|| old.toggle_shortcut.clone());
    let edit = patch
        .edit_shortcut
        .clone()
        .unwrap_or_else(|| old.edit_shortcut.clone());
    let shortcuts_changed =
        hold != old.hold_shortcut || toggle != old.toggle_shortcut || edit != old.edit_shortcut;
    if shortcuts_changed {
        let next = Accelerators {
            hold: &hold,
            toggle: &toggle,
            edit: edit.trim(),
        };
        if let Err(error) = shortcuts::register(&app, next) {
            restore_login_item(&app, login).await;
            return Err(error);
        }
    }

    let new = match db.update_settings(patch).await {
        Ok(new) => new,
        Err(error) => {
            if shortcuts_changed {
                if let Err(restore) = shortcuts::register(&app, Accelerators::of(&old)) {
                    tracing::warn!(%restore, "could not restore the previous shortcuts");
                }
            }
            restore_login_item(&app, login).await;
            return Err(error);
        }
    };

    apply_changes(&app, &old, &new);
    events::emit(&app, events::SETTINGS_CHANGED, new.clone());
    Ok(new)
}

fn apply_changes(app: &AppHandle, old: &Settings, new: &Settings) {
    if old.show_in_dock != new.show_in_dock {
        dock::apply_dock(app, new.show_in_dock);
    }
    if old.show_bar_always != new.show_bar_always {
        overlay::set_always(app, new.show_bar_always);
    }
    let model = &new.transcription_model;
    if old.transcription_model != *model && LOCAL_TRANSCRIPTION_MODELS.contains(&model.as_str()) {
        let app = app.clone();
        let model = model.clone();
        tauri::async_runtime::spawn(async move { models::preload(&app, &model).await });
    }
}

async fn restore_login_item(app: &AppHandle, changed_to: Option<bool>) {
    if let Some(enabled) = changed_to {
        if let Err(error) = login_item::set(app, !enabled).await {
            tracing::warn!(%error, "could not restore the previous login item");
        }
    }
}

/// Makes the setting match macOS, so switching Simple Voice off under Open at Login in System
/// Settings switches the toggle off too. Runs when the helper connects and whenever the main
/// window gains focus. Development builds cannot register a login item and are left alone.
pub(crate) async fn follow_login_item(app: &AppHandle) {
    let state = app.state::<AppState>();
    let _guard = state.login_item.lock().await;
    let mut status = match state.engine.login_item().await {
        Ok(status) => status,
        Err(error) => {
            tracing::debug!(%error, "login item status unavailable");
            return;
        }
    };
    let Ok(db) = state.db() else { return };
    let settings = match db.settings().await {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "could not read settings to follow the login item");
            return;
        }
    };
    // Until now an older version's LaunchAgent carried the setting; hand it over once.
    if login_item::remove_legacy_agent()
        && settings.launch_at_login
        && status != LoginItemStatus::Enabled
    {
        status = match login_item::set(app, true).await {
            Ok(()) => LoginItemStatus::Enabled,
            Err(error) => {
                tracing::warn!(%error, "could not move launch at login to the login item");
                status
            }
        };
    }
    let enabled = status == LoginItemStatus::Enabled;
    if enabled == settings.launch_at_login {
        return;
    }
    let patch = SettingsPatch {
        launch_at_login: Some(enabled),
        ..SettingsPatch::default()
    };
    match db.update_settings(patch).await {
        Ok(settings) => events::emit(app, events::SETTINGS_CHANGED, settings),
        Err(error) => tracing::warn!(%error, "could not store the login item status"),
    }
}

#[tauri::command]
pub(crate) async fn set_groq_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    key: Option<String>,
) -> AppResult<Settings> {
    let db = state.db()?;
    db.set_groq_api_key(normalize_key(key)).await?;
    settings_changed(&app, db.settings().await?)
}

#[tauri::command]
pub(crate) async fn verify_groq_api_key(state: State<'_, AppState>, key: String) -> AppResult<()> {
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::invalid("Enter a Groq API key"));
    }
    sv_cloud::groq_verify_key(&state.http, key).await
}

#[tauri::command]
pub(crate) async fn set_custom_api_key(
    app: AppHandle,
    state: State<'_, AppState>,
    key: Option<String>,
) -> AppResult<Settings> {
    let db = state.db()?;
    db.set_custom_api_key(normalize_key(key)).await?;
    settings_changed(&app, db.settings().await?)
}

#[tauri::command]
pub(crate) async fn test_post_processing(app: AppHandle) -> AppResult<PostProcessingTest> {
    dictation::test_post_processing(&app).await
}

#[tauri::command]
pub(crate) async fn set_database_dir(
    app: AppHandle,
    state: State<'_, AppState>,
    dir: String,
) -> AppResult<Settings> {
    let dir = dir.trim();
    if dir.is_empty() {
        return Err(AppError::invalid("Choose a folder for the database"));
    }
    let db = state.db()?;
    db.relocate(Path::new(dir)).await?;
    settings_changed(&app, db.settings().await?)
}

// Async so it runs off the main thread: registration waits on the main thread.
#[tauri::command]
pub(crate) async fn suspend_shortcuts(app: AppHandle, suspended: bool) -> AppResult<()> {
    shortcuts::suspend(&app, suspended)
}

#[tauri::command]
pub(crate) async fn list_microphones() -> AppResult<Vec<Microphone>> {
    tauri::async_runtime::spawn_blocking(sv_audio::list_microphones)
        .await
        .map_err(AppError::audio)?
}

#[tauri::command]
pub(crate) async fn preview_sound(state: State<'_, AppState>, theme: SoundTheme) -> AppResult<()> {
    let volume = match state.db() {
        Ok(db) => db
            .settings()
            .await
            .map_or(Settings::default().sound_volume, |s| s.sound_volume),
        Err(_) => Settings::default().sound_volume,
    };
    state.play_theme(theme, Cue::Start, volume);
    tokio::time::sleep(PREVIEW_GAP).await;
    state.play_theme(theme, Cue::Stop, volume);
    Ok(())
}

#[tauri::command]
pub(crate) async fn app_info(app: AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let database_path = match state.db() {
        Ok(db) => db.database_path().await.to_string_lossy().into_owned(),
        Err(_) => String::new(),
    };
    let engine_version = lock(&state.engine_version).clone();
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: sv_storage::paths::data_dir()?
            .to_string_lossy()
            .into_owned(),
        database_path,
        models_dir: sv_storage::paths::models_dir()?
            .to_string_lossy()
            .into_owned(),
        engine_version,
    })
}

fn normalize_key(key: Option<String>) -> Option<String> {
    key.map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

fn settings_changed(app: &AppHandle, settings: Settings) -> AppResult<Settings> {
    events::emit(app, events::SETTINGS_CHANGED, settings.clone());
    Ok(settings)
}
