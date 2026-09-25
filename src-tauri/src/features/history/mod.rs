// FilePath: src-tauri/src/features/history/mod.rs
//! Dictation history: list, search, delete, retry failed entries, copy text.

use std::path::Path;

use sv_domain::{AppResult, HistoryEntry};
use tauri::{AppHandle, State};

use crate::events;
use crate::features::dictation;
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
    events::history_changed(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn clear_history(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    for audio in state.db()?.clear_history().await? {
        remove_audio(&audio).await;
    }
    events::history_changed(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn retry_history(app: AppHandle, id: i64) -> AppResult<HistoryEntry> {
    dictation::retry(&app, id).await
}

#[tauri::command]
pub(crate) async fn copy_text(text: String) -> AppResult<()> {
    dictation::copy_to_clipboard(&text)
}

/// Removes a retained WAV. A file that is already gone is the desired end state.
pub(crate) async fn remove_audio(path: impl AsRef<Path>) {
    let path = path.as_ref();
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, path = %path.display(), "could not delete audio"),
    }
}
