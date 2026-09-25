// FilePath: src-tauri/src/features/dictionary/mod.rs
//! Personal dictionary: words that bias recognition and `phrase -> replacement` rules.

use sv_domain::{AppError, AppResult, DictionaryEntry, ImportSummary};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub(crate) async fn list_dictionary(state: State<'_, AppState>) -> AppResult<Vec<DictionaryEntry>> {
    state.db()?.dictionary().await
}

#[tauri::command]
pub(crate) async fn add_dictionary_entry(
    state: State<'_, AppState>,
    phrase: String,
    replacement: Option<String>,
) -> AppResult<DictionaryEntry> {
    state.db()?.add_dictionary_entry(phrase, replacement).await
}

#[tauri::command]
pub(crate) async fn update_dictionary_entry(
    state: State<'_, AppState>,
    id: i64,
    phrase: String,
    replacement: Option<String>,
) -> AppResult<DictionaryEntry> {
    state
        .db()?
        .update_dictionary_entry(id, phrase, replacement)
        .await
}

#[tauri::command]
pub(crate) async fn delete_dictionary_entry(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    state.db()?.delete_dictionary_entry(id).await
}

/// Imports a vocabulary file in the Hammerspoon format: bare words, `heard -> written` rules,
/// `#` comments.
#[tauri::command]
pub(crate) async fn import_vocabulary(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<ImportSummary> {
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| AppError::io(format!("{path}: {error}")))?;
    state.db()?.import_vocabulary(&text).await
}
