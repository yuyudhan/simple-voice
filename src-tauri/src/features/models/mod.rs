// FilePath: src-tauri/src/features/models/mod.rs
//! Settings → Transcription and Settings → Formatting: the model catalog with live status,
//! downloads and deletion of local models, and Smart Select's plan and preloading.

mod catalog;
pub(crate) mod smart_select;

use std::time::Duration;

use sv_domain::models::{model_name, APPLE_SPEECH, LOCAL_TRANSCRIPTION_MODELS, SMART_SELECT};
use sv_domain::{
    AppError, AppResult, ModelInfo, ModelProgress, ModelProgressStatus, ModelStatus, Settings,
    SmartSelectPlan,
};
use tauri::{AppHandle, Manager, State};

use self::catalog::{CatalogEntry, CATALOG};
use crate::events;
use crate::state::{lock, AppState};

/// Longest a status check may take; a helper that cannot answer that fast counts as not ready.
const STATUS_TIMEOUT: Duration = Duration::from_secs(3);

#[tauri::command]
pub(crate) async fn list_models(state: State<'_, AppState>) -> AppResult<Vec<ModelInfo>> {
    let settings = state.db()?.settings().await?;
    let mut models = Vec::with_capacity(CATALOG.len());
    for entry in CATALOG {
        models.push(model_info(&state, &settings, entry).await);
    }
    Ok(models)
}

async fn model_info(state: &AppState, settings: &Settings, entry: &CatalogEntry) -> ModelInfo {
    let mut info = ModelInfo {
        id: entry.id.to_owned(),
        kind: entry.kind,
        provider: entry.provider,
        name: model_name(entry.id).unwrap_or(entry.id).to_owned(),
        subtitle: entry.subtitle.to_owned(),
        speed: entry.speed,
        accuracy: entry.accuracy,
        languages: entry.languages.to_owned(),
        size_mb: entry.size_mb,
        status: ModelStatus::Cloud,
        reason: None,
        progress: None,
    };
    if entry.cloud {
        return info;
    }
    let downloading = lock(&state.downloads).get(entry.id).copied();
    if let Some(fraction) = downloading {
        info.status = ModelStatus::Downloading;
        info.progress = Some(fraction);
        return info;
    }
    let language = engine_language(settings, entry.id);
    let status = tokio::time::timeout(
        STATUS_TIMEOUT,
        state.engine.model_status(entry.id, language),
    )
    .await;
    match status {
        Ok(Ok(status)) => {
            info.status = status.status;
            info.reason = status.reason;
            if let Some(bytes) = status.size_bytes {
                info.size_mb = u32::try_from(bytes / 1_000_000).ok().or(info.size_mb);
            }
        }
        Ok(Err(error)) => {
            info.status = ModelStatus::Unsupported;
            info.reason = Some(format!("The on-device engine is unavailable: {error}"));
        }
        Err(_) => {
            info.status = ModelStatus::Unsupported;
            info.reason = Some("The on-device engine did not respond".to_owned());
        }
    }
    info
}

/// Apple Speech assets are per locale; every other model ignores the language.
pub(crate) fn engine_language<'a>(settings: &'a Settings, model: &str) -> Option<&'a str> {
    if model == APPLE_SPEECH {
        settings.languages.first().map(String::as_str)
    } else {
        None
    }
}

/// Whether a local model can transcribe right now (downloaded and supported on this Mac).
pub(crate) async fn is_ready(state: &AppState, settings: &Settings, model: &str) -> bool {
    if lock(&state.downloads).contains_key(model) {
        return false;
    }
    let language = engine_language(settings, model);
    let status = tokio::time::timeout(STATUS_TIMEOUT, state.engine.model_status(model, language));
    matches!(status.await, Ok(Ok(status)) if status.status == ModelStatus::Ready)
}

/// What Smart Select runs for the current languages, for its card in Settings → Transcription.
#[tauri::command]
pub(crate) async fn smart_select_plan(state: State<'_, AppState>) -> AppResult<SmartSelectPlan> {
    let settings = state.db()?.settings().await?;
    Ok(smart_select::plan(
        &settings.languages,
        settings.groq_api_key_present,
    ))
}

#[tauri::command]
pub(crate) async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    if !LOCAL_TRANSCRIPTION_MODELS.contains(&id.as_str()) {
        return Err(AppError::invalid(format!("{id} cannot be downloaded")));
    }
    let settings = state.db()?.settings().await?;
    {
        let mut downloads = lock(&state.downloads);
        if downloads.contains_key(&id) {
            return Err(AppError::invalid("This model is already downloading"));
        }
        downloads.insert(id.clone(), 0.0);
    }
    progress(&app, &id, 0.0, ModelProgressStatus::Downloading, None);

    let language = engine_language(&settings, &id).map(str::to_owned);
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let reporter = {
            let app = app.clone();
            let id = id.clone();
            Box::new(move |fraction: f32, message: Option<String>| {
                let fraction = fraction.clamp(0.0, 1.0);
                if let Some(entry) = lock(&app.state::<AppState>().downloads).get_mut(&id) {
                    *entry = fraction;
                }
                progress(
                    &app,
                    &id,
                    fraction,
                    ModelProgressStatus::Downloading,
                    message,
                );
            })
        };
        let result = state
            .engine
            .download_model(&id, language.as_deref(), reporter)
            .await;
        lock(&state.downloads).remove(&id);
        match result {
            Ok(()) => {
                progress(&app, &id, 1.0, ModelProgressStatus::Ready, None);
                // A freshly downloaded model the next dictation will use should be warm for it.
                if let Ok(db) = state.db() {
                    if db
                        .settings()
                        .await
                        .is_ok_and(|s| preload_target(&s) == Some(id.as_str()))
                    {
                        preload(&app, &id).await;
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, model = %id, "model download failed");
                progress(
                    &app,
                    &id,
                    0.0,
                    ModelProgressStatus::Failed,
                    Some(error.to_string()),
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub(crate) async fn delete_model(state: State<'_, AppState>, id: String) -> AppResult<()> {
    if !LOCAL_TRANSCRIPTION_MODELS.contains(&id.as_str()) {
        return Err(AppError::invalid(format!(
            "{id} has no local files to delete"
        )));
    }
    // Smart Select is never a model id, so its models stay deletable; it skips missing ones.
    if state.db()?.settings().await?.transcription_model == id {
        return Err(AppError::invalid(
            "This model is in use. Choose another transcription model before deleting it.",
        ));
    }
    if lock(&state.downloads).contains_key(&id) {
        return Err(AppError::invalid("This model is still downloading"));
    }
    state.engine.delete_model(&id).await
}

/// The local model the next dictation will use: the chosen model, or Smart Select's local
/// model for the languages. `None` for Groq Whisper.
pub(crate) fn preload_target(settings: &Settings) -> Option<&str> {
    let model = settings.transcription_model.as_str();
    if model == SMART_SELECT {
        Some(smart_select::local_model(&settings.languages))
    } else {
        LOCAL_TRANSCRIPTION_MODELS.contains(&model).then_some(model)
    }
}

/// Loads the local model the settings will start with, so the first dictation does not pay
/// the load time.
pub(crate) async fn preload_selected(app: &AppHandle, settings: &Settings) {
    if let Some(model) = preload_target(settings) {
        preload(app, model).await;
    }
}

/// Loads a local model into memory so the first dictation does not pay the load time.
pub(crate) async fn preload(app: &AppHandle, model: &str) {
    if let Err(error) = app.state::<AppState>().engine.preload(model).await {
        tracing::info!(%error, model, "model preload skipped");
    }
}

fn progress(
    app: &AppHandle,
    id: &str,
    fraction: f32,
    status: ModelProgressStatus,
    message: Option<String>,
) {
    let payload = ModelProgress {
        id: id.to_owned(),
        fraction,
        status,
        message,
    };
    events::emit(app, events::MODEL_PROGRESS, payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_catalog_entry_has_a_display_name() {
        for entry in CATALOG {
            assert!(
                model_name(entry.id).is_some(),
                "{} has no display name",
                entry.id
            );
        }
    }

    fn settings(model: &str, languages: &[&str]) -> Settings {
        Settings {
            transcription_model: model.to_owned(),
            languages: languages.iter().map(|code| (*code).to_owned()).collect(),
            ..Settings::default()
        }
    }

    #[test]
    fn preload_follows_the_chosen_model_or_the_smart_select_route() {
        use sv_domain::models::{GROQ_WHISPER, PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH};
        assert_eq!(
            preload_target(&settings(PARAKEET_TDT_V3, &["en"])),
            Some(PARAKEET_TDT_V3)
        );
        assert_eq!(preload_target(&settings(GROQ_WHISPER, &["en"])), None);
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en", "hi"])),
            Some(WHISPER_HINGLISH)
        );
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en"])),
            Some(PARAKEET_TDT_V2)
        );
    }
}
