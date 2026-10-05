// FilePath: mobile/src-tauri/src/features/history/mod.rs
//! Dictation history: list, search, delete, retry failed entries, copy text.

use std::path::Path;

use sv_domain::{AppError, AppResult, HistoryEntry};
use sv_mobile::Delivery;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::events;
use crate::state::AppState;

#[tauri::command]
pub(crate) async fn list_history(
    state: State<'_, AppState>,
    query: Option<String>,
    limit: i64,
    before_id: Option<i64>,
) -> AppResult<Vec<HistoryEntry>> {
    let query = query.filter(|query| !query.trim().is_empty());
    state.db()?.list_history(query, limit, before_id).await
}

#[tauri::command]
pub(crate) async fn delete_history(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    if let Some(audio) = state.db()?.delete_history(id).await? {
        remove_audio(&audio).await;
    }
    events::emit(&app, events::HISTORY_CHANGED, ());
    Ok(())
}

/// Re-runs a failed dictation from its kept audio. The field it was meant for is gone, so the
/// result is copied to the clipboard.
#[tauri::command]
pub(crate) async fn retry_history(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<HistoryEntry> {
    let result = sv_mobile::retry(state.db()?, state.http()?, id, Delivery::Copied).await;
    events::emit(&app, events::HISTORY_CHANGED, ());
    let (entry, text) = result?;
    app.clipboard().write_text(text).map_err(AppError::other)?;
    Ok(entry)
}

#[tauri::command]
pub(crate) async fn copy_text(app: AppHandle, text: String) -> AppResult<()> {
    app.clipboard().write_text(text).map_err(AppError::other)
}

/// Removes a retained recording. A file that is already gone is the desired end state.
async fn remove_audio(path: impl AsRef<Path>) {
    let path = path.as_ref();
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, path = %path.display(), "could not delete audio"),
    }
}
