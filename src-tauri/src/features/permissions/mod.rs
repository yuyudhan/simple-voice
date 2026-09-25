// FilePath: src-tauri/src/features/permissions/mod.rs
//! Microphone, Accessibility and Speech Recognition permissions. The engine helper owns the
//! checks (they need Apple frameworks); this slice caches the last answer and tells the UI when
//! it changes.

use std::time::Duration;

use sv_domain::{AppResult, PermissionKind, Permissions};
use tauri::{AppHandle, Manager, State};

use crate::events;
use crate::platform::windows::MAIN;
use crate::state::{lock, AppState};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[tauri::command]
pub(crate) async fn get_permissions(app: AppHandle) -> AppResult<Permissions> {
    refresh(&app).await
}

#[tauri::command]
pub(crate) async fn request_permission(
    app: AppHandle,
    state: State<'_, AppState>,
    kind: PermissionKind,
) -> AppResult<Permissions> {
    let permissions = state.engine.request_permission(kind).await?;
    observe(&app, permissions);
    Ok(permissions)
}

#[tauri::command]
pub(crate) async fn open_permission_settings(
    state: State<'_, AppState>,
    kind: PermissionKind,
) -> AppResult<()> {
    state.engine.open_settings(kind).await
}

/// Asks the helper for the current permissions and emits `permissions-changed` on change.
pub(crate) async fn refresh(app: &AppHandle) -> AppResult<Permissions> {
    let permissions = app.state::<AppState>().engine.permissions().await?;
    observe(app, permissions);
    Ok(permissions)
}

/// Like `refresh`, but always emits: a failed paste must bring the Accessibility prompt in the
/// UI back even when the cached status already said "denied".
pub(crate) async fn announce(app: &AppHandle) -> AppResult<Permissions> {
    let permissions = app.state::<AppState>().engine.permissions().await?;
    if !observe(app, permissions) {
        events::emit(app, events::PERMISSIONS_CHANGED, permissions);
    }
    Ok(permissions)
}

/// Records permissions learned elsewhere (e.g. the pre-dictation check) and emits
/// `permissions-changed` when they differ from the last known ones. Returns whether it emitted.
pub(crate) fn observe(app: &AppHandle, permissions: Permissions) -> bool {
    let changed = {
        let state = app.state::<AppState>();
        let mut last = lock(&state.permissions);
        let changed = last.as_ref() != Some(&permissions);
        *last = Some(permissions);
        changed
    };
    if changed {
        events::emit(app, events::PERMISSIONS_CHANGED, permissions);
    }
    changed
}

/// Permissions are granted in System Settings, outside the app, so they are polled while the
/// user is looking at the main window (onboarding, Settings → Permissions).
pub(crate) fn spawn_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let focused = app
                .get_webview_window(MAIN)
                .is_some_and(|window| window.is_focused().unwrap_or(false));
            if focused {
                if let Err(error) = refresh(&app).await {
                    tracing::debug!(%error, "permission poll failed");
                }
            }
        }
    });
}
