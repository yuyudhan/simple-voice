// FilePath: src-tauri/src/features/snippets/mod.rs
//! Personal snippets: spoken triggers that dictation expands into saved text (see
//! `sv_text::snippets`). The commands are the Snippets page's CRUD; every change emits
//! `snippets-changed` so open windows reload.

use sv_domain::{AppResult, Snippet};
use tauri::{AppHandle, State};

use crate::events;
use crate::state::AppState;

#[tauri::command]
pub(crate) async fn list_snippets(state: State<'_, AppState>) -> AppResult<Vec<Snippet>> {
    state.db()?.snippets().await
}

#[tauri::command]
pub(crate) async fn add_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    trigger: String,
    text: String,
) -> AppResult<Snippet> {
    let snippet = state.db()?.add_snippet(trigger, text).await?;
    events::emit(&app, events::SNIPPETS_CHANGED, ());
    Ok(snippet)
}

#[tauri::command]
pub(crate) async fn update_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    trigger: String,
    text: String,
) -> AppResult<Snippet> {
    let snippet = state.db()?.update_snippet(id, trigger, text).await?;
    events::emit(&app, events::SNIPPETS_CHANGED, ());
    Ok(snippet)
}

#[tauri::command]
pub(crate) async fn delete_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    state.db()?.delete_snippet(id).await?;
    events::emit(&app, events::SNIPPETS_CHANGED, ());
    Ok(())
}

/// The snippets a dictation expands. A database error only means this dictation expands
/// nothing, so it is logged instead of failing the dictation.
pub(crate) async fn load(state: &AppState) -> Vec<Snippet> {
    let snippets = match state.db() {
        Ok(db) => db.snippets().await,
        Err(error) => Err(error),
    };
    snippets.unwrap_or_else(|error| {
        tracing::warn!(%error, "could not load snippets; expanding none");
        Vec::new()
    })
}
