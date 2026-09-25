// FilePath: src-tauri/src/features/models/mod.rs
//! Settings → Models: the catalog with live status, downloads and deletion of local models.

mod catalog;

use std::time::Duration;

use sv_domain::models::{APPLE_SPEECH, LOCAL_TRANSCRIPTION_MODELS};
use sv_domain::{
    AppError, AppResult, ModelInfo, ModelProgress, ModelProgressStatus, ModelStatus, Settings,
};
use tauri::{AppHandle, Manager, State};

use self::catalog::{CatalogEntry, CATALOG};
use crate::events;
use crate::state::{lock, AppState};

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
        name: entry.name.to_owned(),
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
                // A freshly downloaded selected model should be warm for the next dictation.
                if let Ok(db) = state.db() {
                    if db
                        .settings()
                        .await
                        .is_ok_and(|s| s.transcription_model == id)
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

/// Loads a local model into memory so the first dictation does not pay the load time.
pub(crate) async fn preload(app: &AppHandle, model: &str) {
    if !LOCAL_TRANSCRIPTION_MODELS.contains(&model) {
        return;
    }
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
