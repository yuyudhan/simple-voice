// FilePath: src-tauri/src/features/dictionary/mod.rs
//! Personal dictionary: words that bias recognition and `phrase -> replacement` rules, plus the
//! built-in vocabulary packs offered beside it.

use sv_domain::{AppError, AppResult, DictionaryEntry, ImportSummary, VocabularyPackInfo};
use tauri::{AppHandle, Manager, State};

use crate::features::models;
use crate::state::AppState;

#[tauri::command]
pub(crate) async fn list_dictionary(state: State<'_, AppState>) -> AppResult<Vec<DictionaryEntry>> {
    state.db()?.dictionary().await
}

/// The built-in packs with their spellings; which are on lives in `Settings.vocabularyPacks`.
#[tauri::command]
pub(crate) fn list_vocabulary_packs() -> Vec<VocabularyPackInfo> {
    sv_text::vocabulary_pack_infos()
}

#[tauri::command]
pub(crate) async fn add_dictionary_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    phrase: String,
    replacement: Option<String>,
) -> AppResult<DictionaryEntry> {
    let entry = state
        .db()?
        .add_dictionary_entry(phrase, replacement)
        .await?;
    fetch_booster(&app);
    Ok(entry)
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
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<ImportSummary> {
    let text = tokio::fs::read_to_string(&path)
        .await
        .map_err(|error| AppError::io(format!("{path}: {error}")))?;
    let summary = state.db()?.import_vocabulary(&text).await?;
    fetch_booster(&app);
    Ok(summary)
}

/// A Dictionary that gains names needs Parakeet's booster to reach the transcriber.
fn fetch_booster(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let settings = match app.state::<AppState>().db() {
            Ok(db) => db.settings().await,
            Err(error) => Err(error),
        };
        match settings {
            Ok(settings) => models::fetch_booster(&app, &settings).await,
            Err(error) => tracing::info!(%error, "dictionary booster check skipped"),
        }
    });
}

/// Writes the dictionary to `path` in the same format `import_vocabulary` reads.
#[tauri::command]
pub(crate) async fn export_vocabulary(state: State<'_, AppState>, path: String) -> AppResult<()> {
    let today = chrono::Local::now().date_naive();
    let text = state.db()?.export_vocabulary(today).await?;
    tokio::fs::write(&path, text)
        .await
        .map_err(|error| AppError::io(format!("{path}: {error}")))
}
