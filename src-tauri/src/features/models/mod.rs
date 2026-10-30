// FilePath: src-tauri/src/features/models/mod.rs
//! Settings → Transcription and Settings → Formatting: the on-device model catalog with live
//! status, user-initiated downloads and deletion, and Smart Select's plan and preloading.

mod catalog;
pub(crate) mod smart_select;

use std::time::Duration;

use sv_domain::languages::model_language;
use sv_domain::models::{
    model_name, APPLE_SPEECH, DOWNLOADABLE_MODELS, LOCAL_TRANSCRIPTION_MODELS, SMART_SELECT,
};
use sv_domain::{
    AppError, AppResult, ModelInfo, ModelKind, ModelProgress, ModelProgressStatus, ModelProvider,
    ModelStatus, Settings, SmartSelectPlan,
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

/// Live status of one model id, as `list_models` reports it. An id outside the catalog is
/// unsupported.
pub(crate) async fn model_status(state: &AppState, settings: &Settings, id: &str) -> ModelInfo {
    match CATALOG.iter().find(|entry| entry.id == id) {
        Some(entry) => model_info(state, settings, entry).await,
        None => ModelInfo {
            id: id.to_owned(),
            kind: ModelKind::PostProcessing,
            provider: ModelProvider::Llm,
            name: model_name(id).unwrap_or(id).to_owned(),
            subtitle: String::new(),
            speed: 0,
            accuracy: 0,
            languages: String::new(),
            size_mb: None,
            status: ModelStatus::Unsupported,
            reason: Some(format!("{id} is not an on-device model")),
            progress: None,
            license: None,
            source: None,
            streaming: false,
        },
    }
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
        status: ModelStatus::Unsupported,
        reason: None,
        progress: None,
        license: None,
        source: None,
        streaming: false,
    };
    let downloading = lock(&state.downloads).get(entry.id).copied();
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
            info.license = status.license;
            info.source = status.source;
            info.streaming = status.streaming;
            if let Some(bytes) = status.size_bytes {
                info.size_mb = u32::try_from(bytes.div_ceil(1_000_000))
                    .ok()
                    .or(info.size_mb);
            }
        }
        Ok(Err(error)) => {
            info.reason = Some(format!("The on-device engine is unavailable: {error}"));
        }
        Err(_) => {
            info.reason = Some("The on-device engine did not respond".to_owned());
        }
    }
    if let Some(fraction) = downloading {
        info.status = ModelStatus::Downloading;
        info.progress = Some(fraction);
    }
    info
}

/// Apple Speech assets are per locale; every other model ignores the language.
pub(crate) fn engine_language<'a>(settings: &'a Settings, model: &str) -> Option<&'a str> {
    if model == APPLE_SPEECH {
        // Apple locales exist per spoken language; a script variant is heard as its base.
        settings.languages.first().map(|code| model_language(code))
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
    let mut ready = Vec::new();
    for model in smart_select::route(&settings.languages) {
        if is_ready(&state, &settings, model).await {
            ready.push(model);
        }
    }
    Ok(smart_select::plan(&settings.languages, |model| {
        ready.contains(&model)
    }))
}

#[tauri::command]
pub(crate) async fn download_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    if !DOWNLOADABLE_MODELS.contains(&id.as_str()) {
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
                events::emit(&app, events::MODELS_CHANGED, ());
                // A freshly downloaded model the next dictation will use should be warm for it.
                if let Ok(db) = state.db() {
                    if db
                        .settings()
                        .await
                        .is_ok_and(|s| preload_target(&s, |model| model == id) == Some(id.as_str()))
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
pub(crate) async fn delete_model(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    if !DOWNLOADABLE_MODELS.contains(&id.as_str()) {
        return Err(AppError::invalid(format!(
            "{id} has no local files to delete"
        )));
    }
    let settings = state.db()?.settings().await?;
    // Smart Select is never a model id, so its models stay deletable; it skips missing ones.
    if settings.transcription_model == id {
        return Err(AppError::invalid(
            "This model is in use. Choose another transcription model before deleting it.",
        ));
    }
    if crate::features::llm::post_processing_model(&settings).as_deref() == Some(id.as_str()) {
        return Err(AppError::invalid(
            "This model formats your dictations. Choose another formatting option first.",
        ));
    }
    if lock(&state.downloads).contains_key(&id) {
        return Err(AppError::invalid("This model is still downloading"));
    }
    state.engine.delete_model(&id).await?;
    lock(&state.llm_used).remove(&id);
    events::emit(&app, events::MODELS_CHANGED, ());
    Ok(())
}

/// The voice model the next dictation runs: the chosen model, or the first model of Smart
/// Select's route that `is_ready` (`None` when none is).
pub(crate) fn preload_target(settings: &Settings, is_ready: impl Fn(&str) -> bool) -> Option<&str> {
    let model = settings.transcription_model.as_str();
    if model == SMART_SELECT {
        smart_select::ready_route(&settings.languages, is_ready)
            .ok()
            .map(|route| route.first)
    } else {
        LOCAL_TRANSCRIPTION_MODELS.contains(&model).then_some(model)
    }
}

/// The voice models that are ready right now among those the settings could run.
async fn ready_models(state: &AppState, settings: &Settings) -> Vec<&'static str> {
    let candidates: Vec<&'static str> = if settings.transcription_model == SMART_SELECT {
        smart_select::route(&settings.languages)
    } else {
        Vec::new()
    };
    let mut ready = Vec::with_capacity(candidates.len());
    for model in candidates {
        if is_ready(state, settings, model).await {
            ready.push(model);
        }
    }
    ready
}

/// The voice model the next dictation (or meeting) runs, judged by what is ready now.
pub(crate) async fn active_model(state: &AppState, settings: &Settings) -> Option<String> {
    let ready = ready_models(state, settings).await;
    preload_target(settings, |model| ready.contains(&model)).map(str::to_owned)
}

/// Loads the local model the settings will start with, so the first dictation does not pay
/// the load time.
pub(crate) async fn preload_selected(app: &AppHandle, settings: &Settings) {
    if let Some(model) = active_model(&app.state::<AppState>(), settings).await {
        preload(app, &model).await;
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
    fn the_catalog_offers_only_on_device_models_and_every_download_is_listed() {
        for entry in CATALOG {
            assert!(
                entry.id == sv_domain::models::APPLE_INTELLIGENCE
                    || DOWNLOADABLE_MODELS.contains(&entry.id),
                "{} is neither built in nor downloadable",
                entry.id
            );
        }
        for id in DOWNLOADABLE_MODELS {
            assert!(
                CATALOG.iter().any(|entry| entry.id == *id),
                "{id} not listed"
            );
        }
        assert!(!CATALOG
            .iter()
            .any(|entry| entry.id == sv_domain::models::GROQ_WHISPER));
    }

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
        use sv_domain::models::{
            PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH, WHISPER_TURBO,
        };
        let all = |_: &str| true;
        assert_eq!(
            preload_target(&settings(PARAKEET_TDT_V3, &["en"]), all),
            Some(PARAKEET_TDT_V3)
        );
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en", "hi", "hi-Latn"]), all),
            Some(WHISPER_HINGLISH)
        );
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en"]), all),
            Some(PARAKEET_TDT_V2)
        );
        // The QA install: Hinglish Whisper not downloaded, Parakeet v3 and Turbo ready. The
        // model that actually runs is preloaded, never the missing recommendation.
        let qa = |model: &str| model == PARAKEET_TDT_V3 || model == WHISPER_TURBO;
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en", "hi", "hi-Latn"]), qa),
            Some(WHISPER_TURBO)
        );
        assert_eq!(
            preload_target(&settings(SMART_SELECT, &["en", "hi", "hi-Latn"]), |_| false),
            None
        );
    }

    #[test]
    fn apple_speech_hears_a_script_variant_as_its_base() {
        use sv_domain::models::PARAKEET_TDT_V3;
        let variant_first = settings(APPLE_SPEECH, &["hi-Latn", "en"]);
        assert_eq!(engine_language(&variant_first, APPLE_SPEECH), Some("hi"));
        assert_eq!(engine_language(&variant_first, PARAKEET_TDT_V3), None);
        let english_first = settings(APPLE_SPEECH, &["en", "hi-Latn"]);
        assert_eq!(engine_language(&english_first, APPLE_SPEECH), Some("en"));
    }
}
