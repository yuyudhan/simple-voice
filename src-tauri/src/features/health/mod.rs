// FilePath: src-tauri/src/features/health/mod.rs
//! The app's health (`sv_domain::health`): what is wrong right now, each issue with its words
//! and its fix, pushed to the UI as `health-changed`, shown in the tray, logged once per change
//! and checked before a dictation starts, so a recording that cannot work is refused with the
//! reason instead of waiting.
//!
//! Facts arrive from where they are observed: the app's own events (engine status, permissions,
//! models, settings), the capture worker, transcription results, shortcut registration, and a
//! poll of the microphone every few seconds while nothing records (a cheap device listing, no
//! stream), which is how "No microphone connected" appears before anyone presses the shortcut
//! and clears itself when one is plugged in.

mod store;

use std::time::Duration;

use serde::de::DeserializeOwned;
use sv_audio::check_input;
use sv_domain::models::{model_name, SMART_SELECT};
use sv_domain::{
    AppError, AppResult, CaptureProblem, EngineState, EngineStatus, Health, InputState, Issue,
    IssueAction, IssueKind, ModelFact, ModelProgress, ModelProgressStatus, ModelStatus,
    Permissions, ProviderFact, Settings, TranscriptionProvider,
};
use tauri::{AppHandle, Listener, Manager, State};
use tauri_plugin_opener::OpenerExt;

pub(crate) use self::store::HealthStore;
use crate::events;
use crate::features::models::{self, smart_select};
use crate::features::{groq, permissions};
use crate::platform::tray;
use crate::state::{lock, AppState};

/// How often the microphone is looked for while nothing records.
const POLL_INTERVAL: Duration = Duration::from_secs(3);
/// Longest a model status check may take before the model counts as unknown.
const MODEL_TIMEOUT: Duration = Duration::from_secs(3);
const TROUBLESHOOTING_URL: &str =
    "https://github.com/yuyudhan/simple-voice/blob/main/docs/troubleshooting.md";
#[cfg(target_os = "macos")]
const SOUND_INPUT_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.sound?input";

/// Starts listening for facts, the microphone poll and the publisher.
pub(crate) fn spawn(app: &AppHandle) {
    let wayland = cfg!(target_os = "linux")
        && std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session == "wayland");
    app.state::<AppState>()
        .health
        .update(|facts| facts.wayland_session = wayland);
    listen::<EngineStatus>(app, events::ENGINE_STATUS, |app, status| {
        let running = status.state == EngineState::Running;
        app.state::<AppState>().health.update(|facts| {
            facts.engine = status;
            if !running {
                facts.model = ModelFact::Unknown;
            }
        });
        if running {
            tauri::async_runtime::spawn(refresh_models(app));
        }
    });
    listen::<Permissions>(app, events::PERMISSIONS_CHANGED, |app, permissions| {
        app.state::<AppState>()
            .health
            .update(|facts| facts.permissions = Some(permissions));
    });
    listen::<ModelProgress>(app, events::MODEL_PROGRESS, |app, progress| {
        if progress.status != ModelProgressStatus::Downloading {
            // Ready and failed downloads are followed by `models-changed` or need a fresh look.
            tauri::async_runtime::spawn(refresh_models(app));
            return;
        }
        app.state::<AppState>().health.update(|facts| {
            let selected = match &facts.model {
                ModelFact::Missing { model, .. } | ModelFact::Downloading { model, .. } => {
                    *model == progress.id
                }
                _ => false,
            };
            if selected {
                facts.model = downloading(&progress.id, progress.fraction);
            }
        });
    });
    for event in [events::MODELS_CHANGED, events::SETTINGS_CHANGED] {
        let app_handle = app.clone();
        app.listen_any(event, move |_| {
            tauri::async_runtime::spawn(refresh_models(app_handle.clone()));
        });
    }
    tauri::async_runtime::spawn(publish(app.clone()));
    tauri::async_runtime::spawn(monitor(app.clone()));
}

/// Calls `handle` with each payload of `event` the app emits.
fn listen<T: DeserializeOwned + 'static>(app: &AppHandle, event: &str, handle: fn(AppHandle, T)) {
    let app_handle = app.clone();
    let name = event.to_owned();
    app.listen_any(event, move |emitted| {
        match serde_json::from_str::<T>(emitted.payload()) {
            Ok(payload) => handle(app_handle.clone(), payload),
            Err(error) => tracing::warn!(%error, event = %name, "unreadable event payload"),
        }
    });
}

/// Derives the health after every change of the facts and announces it when it differs.
async fn publish(app: AppHandle) {
    let state = app.state::<AppState>();
    loop {
        state.health.changed().await;
        let health = state.health.current();
        if !state.health.publish(&health) {
            continue;
        }
        if health.issues.is_empty() {
            tracing::info!("health: all clear");
        }
        for issue in &health.issues {
            tracing::warn!(kind = ?issue.kind, severity = ?issue.severity, "health: {}", issue.sentence());
        }
        tray::set_health(&app, &health);
        events::emit(&app, events::HEALTH_CHANGED, health);
    }
}

/// Looks for the microphone and at the engine every few seconds, and rechecks permissions while
/// one is refused, so every issue clears on its own once the cause is gone.
async fn monitor(app: AppHandle) {
    loop {
        recheck(&app, false).await;
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// The engine saw a microphone come or go (Core Audio on macOS, `pactl subscribe` on Linux):
/// check now.
pub(crate) fn devices_changed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if !app.state::<AppState>().dictation.is_recording() {
            refresh_input(&app).await;
        }
    });
}

/// One round of checks. `now` also checks while recording (a refused start just failed).
async fn recheck(app: &AppHandle, now: bool) {
    let state = app.state::<AppState>();
    let stalled = state.engine.stalled();
    state.health.update(|facts| facts.engine_stalled = stalled);
    if now || !state.dictation.is_recording() {
        refresh_input(app).await;
    }
    let permission_issue = state.health.current().issues.iter().any(|issue| {
        matches!(
            issue.kind,
            IssueKind::MicrophoneDenied
                | IssueKind::MicrophoneNotAsked
                | IssueKind::AccessibilityDenied
        )
    });
    if now || permission_issue {
        if let Err(error) = permissions::refresh(app).await {
            tracing::debug!(%error, "permission recheck failed");
        }
    }
}

async fn settings(state: &AppState) -> Option<Settings> {
    match state.db() {
        Ok(db) => db.settings().await.ok(),
        Err(_) => None,
    }
}

/// Looks for the microphone now. A change in what is connected also clears the last capture
/// problem: it was about a device that is gone or back.
async fn refresh_input(app: &AppHandle) {
    let state = app.state::<AppState>();
    let requested = settings(&state)
        .await
        .and_then(|settings| settings.microphone);
    let checked =
        tauri::async_runtime::spawn_blocking(move || check_input(requested.as_deref())).await;
    let input = match checked {
        Ok(input) => input,
        Err(error) => {
            tracing::debug!(%error, "input check did not finish");
            return;
        }
    };
    state.health.update(|facts| {
        if facts.input != input {
            if facts.input != InputState::Unknown {
                facts.capture = None;
            }
            facts.input = input;
        }
    });
}

/// The selected voice model and the cloud provider, as transcription would use them now.
async fn refresh_models(app: AppHandle) {
    let state = app.state::<AppState>();
    let Some(settings) = settings(&state).await else {
        return;
    };
    let provider = match settings.transcription_provider {
        TranscriptionProvider::Local => ProviderFact::Local,
        TranscriptionProvider::Groq => {
            let has_key = matches!(groq::key(&state).await, Ok(Some(_)));
            let fault = match state.health.facts().provider {
                ProviderFact::Cloud { fault, .. } => fault,
                ProviderFact::Local => None,
            };
            ProviderFact::Cloud {
                provider: "Groq".to_owned(),
                has_key,
                fault,
            }
        }
    };
    let running = state.health.facts().engine.state == EngineState::Running;
    let model = if running {
        model_fact(&state, &settings).await
    } else {
        ModelFact::Unknown
    };
    state.health.update(|facts| {
        facts.provider = provider;
        // A failure seen in use stays until that model works again or another is chosen.
        let failed_same = matches!(
            (&facts.model, &model),
            (ModelFact::Failed { model: failed, .. }, ModelFact::Ready)
                if Some(failed.as_str()) == fixed_model(&settings)
        );
        if !failed_same {
            facts.model = model;
        }
    });
}

fn fixed_model(settings: &Settings) -> Option<&str> {
    let model = settings.transcription_model.as_str();
    (model != SMART_SELECT).then_some(model)
}

fn named(model: &str) -> String {
    model_name(model).unwrap_or(model).to_owned()
}

async fn model_fact(state: &AppState, settings: &Settings) -> ModelFact {
    let Some(model) = fixed_model(settings) else {
        for candidate in smart_select::route(&settings.languages) {
            if models::is_ready(state, settings, candidate).await {
                return ModelFact::Ready;
            }
        }
        // Nothing ready: Smart Select needs the best model for the languages.
        let best = smart_select::local_model(&settings.languages);
        return match lock(&state.downloads).get(best).copied() {
            Some(fraction) => downloading(best, fraction),
            None => ModelFact::Missing {
                model: best.to_owned(),
                name: named(best),
            },
        };
    };
    if let Some(fraction) = lock(&state.downloads).get(model).copied() {
        return downloading(model, fraction);
    }
    let language = models::engine_language(settings, model);
    let status = tokio::time::timeout(MODEL_TIMEOUT, state.engine.model_status(model, language));
    let Ok(Ok(status)) = status.await else {
        // The engine did not say; its own issue covers a helper that does not answer.
        return ModelFact::Unknown;
    };
    match status.status {
        ModelStatus::Ready => ModelFact::Ready,
        ModelStatus::Downloading => downloading(model, 0.0),
        ModelStatus::NotDownloaded => ModelFact::Missing {
            model: model.to_owned(),
            name: named(model),
        },
        ModelStatus::Unsupported => ModelFact::Unavailable {
            model: model.to_owned(),
            name: named(model),
            reason: status
                .reason
                .unwrap_or_else(|| "It is not supported on this computer".to_owned()),
        },
    }
}

fn downloading(model: &str, fraction: f32) -> ModelFact {
    ModelFact::Downloading {
        model: model.to_owned(),
        name: named(model),
        percent: (fraction.clamp(0.0, 1.0) * 100.0).round() as u8,
    }
}

/// A recording could not start, or reported a problem while it ran.
pub(crate) fn capture_problem(state: &AppState, problem: &CaptureProblem) {
    let problem = problem.clone();
    state.health.update(|facts| {
        if problem == CaptureProblem::NoDevice {
            facts.input = InputState::NoDevice;
        }
        facts.capture = Some(problem);
    });
}

/// A recording ended; one that had no problem clears the last one.
pub(crate) fn capture_finished(state: &AppState, problem: Option<&CaptureProblem>) {
    let problem = problem.cloned();
    state.health.update(|facts| facts.capture = problem);
}

/// The outcome of transcribing with a local model.
pub(crate) fn model_outcome(state: &AppState, model: &str, error: Option<&AppError>) {
    match error {
        // A timeout is the engine's issue (not responding), not the model's.
        Some(AppError::Engine(reason)) if state.engine.stalled().is_none() => {
            let reason = reason.clone();
            state.health.update(|facts| {
                if facts.model == ModelFact::Ready {
                    facts.model = ModelFact::Failed {
                        model: model.to_owned(),
                        name: named(model),
                        reason,
                    };
                }
            });
        }
        Some(_) => {}
        None => state.health.update(|facts| {
            if matches!(&facts.model, ModelFact::Failed { model: failed, .. } if failed == model) {
                facts.model = ModelFact::Ready;
            }
        }),
    }
}

/// The outcome of a cloud transcription: a fault the user can fix, or a success that clears it.
pub(crate) fn provider_outcome(state: &AppState, error: Option<&AppError>) {
    let fault = error.and_then(AppError::provider_fault);
    if error.is_some() && fault.is_none() {
        return;
    }
    state.health.update(|facts| {
        if let ProviderFact::Cloud { fault: current, .. } = &mut facts.provider {
            *current = fault;
        }
    });
}

/// The cloud key was saved (`true`) or removed.
pub(crate) fn provider_key(state: &AppState, saved: bool) {
    state.health.update(|facts| {
        if let ProviderFact::Cloud { has_key, fault, .. } = &mut facts.provider {
            *has_key = saved;
            *fault = None;
        }
    });
}

/// Registering the dictation shortcuts failed on `shortcut`, or (`None`) succeeded.
pub(crate) fn shortcut_taken(state: &AppState, shortcut: Option<&str>) {
    let shortcut = shortcut.map(str::to_owned);
    state.health.update(|facts| facts.shortcut_taken = shortcut);
}

/// The issue that refuses a dictation now, if any. A missing microphone is looked for again
/// first: it may have been plugged in since the last poll.
pub(crate) async fn blocking_issue(app: &AppHandle) -> Option<Issue> {
    let state = app.state::<AppState>();
    let issue = state.health.current().blocking().cloned()?;
    if matches!(
        issue.kind,
        IssueKind::NoMicrophone | IssueKind::NoSoundServer
    ) {
        refresh_input(app).await;
        return state.health.current().blocking().cloned();
    }
    Some(issue)
}

#[tauri::command]
pub(crate) fn get_health(state: State<'_, AppState>) -> Health {
    state.health.current()
}

/// Runs the actions that need the app's side: system settings panes, the troubleshooting
/// guide, and a recheck. `kind` is the issue's `kind`, naming the guide's section.
#[tauri::command]
pub(crate) async fn health_action(
    app: AppHandle,
    action: IssueAction,
    kind: String,
) -> AppResult<Health> {
    match action {
        IssueAction::Recheck => {
            app.state::<AppState>()
                .health
                .update(|facts| facts.capture = None);
            recheck(&app, true).await;
        }
        IssueAction::OpenHelp => {
            let section = kind.replace('_', "-");
            open_url(&app, &format!("{TROUBLESHOOTING_URL}#{section}"))?;
        }
        IssueAction::OpenSoundSettings => open_sound_settings(&app).await?,
        other => {
            return Err(AppError::invalid(format!(
                "{other:?} is handled by the window, not the app"
            )))
        }
    }
    Ok(app.state::<AppState>().health.current())
}

fn open_url(app: &AppHandle, url: &str) -> AppResult<()> {
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| AppError::other(format!("Could not open {url}: {error}")))
}

#[cfg(target_os = "macos")]
async fn open_sound_settings(app: &AppHandle) -> AppResult<()> {
    open_url(app, SOUND_INPUT_SETTINGS)
}

/// The Linux helper opens the desktop's sound settings for the microphone.
#[cfg(not(target_os = "macos"))]
async fn open_sound_settings(app: &AppHandle) -> AppResult<()> {
    app.state::<AppState>()
        .engine
        .open_settings(sv_domain::PermissionKind::Microphone)
        .await
}
