// FilePath: src-tauri/src/features/insights/mod.rs
//! Usage statistics for the Insights page.

use sv_domain::{AppResult, Insights, ModelInsights};
use tauri::State;

use crate::state::{now_ms, AppState};

#[tauri::command]
pub(crate) async fn get_insights(state: State<'_, AppState>) -> AppResult<Insights> {
    // Streaks and the heatmap are per local calendar day.
    let utc_offset_minutes = chrono::Local::now().offset().local_minus_utc() / 60;
    state.db()?.insights(now_ms(), utc_offset_minutes).await
}

#[tauri::command]
pub(crate) async fn get_model_insights(state: State<'_, AppState>) -> AppResult<ModelInsights> {
    state.db()?.model_insights().await
}
