// FilePath: src-tauri/src/features/llm.rs
//! One request to an on-device language model through the engine helper: Apple Intelligence or
//! the downloaded local model. Dictation formatting, edit mode, learning and meetings all call
//! it and judge the reply with their own checks. Nothing here reaches the network.

use std::time::{Duration, Instant};

use sv_domain::models::APPLE_INTELLIGENCE;
use sv_domain::{LlmBackend, ModelStatus, PostProcessing, Settings};
use sv_engine::{GenerateRequest, ProgressCallback};
use sv_text::{PolishPrompt, PolishTarget};

use tauri::{AppHandle, Manager};

use crate::features::models::model_status;
use crate::state::{lock, AppState};

/// On-device generation is slower than the hosted models the per-pass budgets in sv-text were
/// first tuned for (Apple Intelligence measured 4.8–8.9 s on the formatting sample, the local
/// model ~1.2 s), so no request gets less than this once its model is loaded.
const WARM_FLOOR: Duration = Duration::from_secs(15);
/// Extra time for a model that is probably not in memory: the local model loads in ~2.6 s from
/// disk; Apple Intelligence's first request after launch measured 34.6 s.
const COLD_START: Duration = Duration::from_secs(45);
/// The helper unloads the local model after 10 minutes without a request; a little earlier
/// than that a model counts as cold again.
const WARM_FOR: Duration = Duration::from_secs(9 * 60);

/// A model reply; `finished` is false when it stopped at the token limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reply {
    pub(crate) text: String,
    pub(crate) finished: bool,
}

/// The model dictation post-processing runs on; `None` when it is off.
pub(crate) fn post_processing_model(settings: &Settings) -> Option<&str> {
    match settings.post_processing {
        PostProcessing::Off => None,
        PostProcessing::Apple => Some(APPLE_INTELLIGENCE),
        PostProcessing::Local => Some(settings.local_llm_model.as_str()),
    }
}

/// The model a backend choice runs on.
pub(crate) fn backend_model(settings: &Settings, backend: LlmBackend) -> &str {
    match backend {
        LlmBackend::Apple => APPLE_INTELLIGENCE,
        LlmBackend::Local => settings.local_llm_model.as_str(),
    }
}

/// The prompt family a model needs: Apple's small on-device model gets the shorter one.
pub(crate) fn target(model: &str) -> PolishTarget {
    if model == APPLE_INTELLIGENCE {
        PolishTarget::OnDevice
    } else {
        PolishTarget::Chat
    }
}

/// Why `model` cannot answer right now, for the user; `None` when it is ready.
pub(crate) async fn unavailable(
    state: &AppState,
    settings: &Settings,
    model: &str,
) -> Option<String> {
    let status = model_status(state, settings, model).await;
    match status.status {
        ModelStatus::Ready => None,
        ModelStatus::NotDownloaded => Some(format!(
            "{} is not downloaded. Download it in Settings → Formatting.",
            status.name
        )),
        ModelStatus::Downloading => Some(format!("{} is still downloading.", status.name)),
        ModelStatus::Unsupported => Some(
            status
                .reason
                .unwrap_or_else(|| format!("{} is not available on this Mac.", status.name)),
        ),
    }
}

/// Sends `prompt` to the dictation post-processing model (formatting, edit mode, learning).
pub(crate) async fn post_process(
    state: &AppState,
    settings: &Settings,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<Reply, String> {
    match post_processing_model(settings) {
        Some(model) => generate(state, model, prompt, max_tokens, limit).await,
        None => Err("AI post-processing is off".to_owned()),
    }
}

/// The prompt family the dictation post-processing model needs.
pub(crate) fn post_processing_target(settings: &Settings) -> PolishTarget {
    post_processing_model(settings).map_or(PolishTarget::Chat, target)
}

/// The time a request to `model` gets, given the caller's budget for the work itself: at least
/// [`WARM_FLOOR`], plus [`COLD_START`] when the model has not answered or loaded recently.
pub(crate) fn budget(state: &AppState, model: &str, work: Duration) -> Duration {
    let warm = lock(&state.llm_used)
        .get(model)
        .is_some_and(|at| at.elapsed() < WARM_FOR);
    budget_for(work, warm)
}

fn budget_for(work: Duration, warm: bool) -> Duration {
    let limit = work.max(WARM_FLOOR);
    if warm {
        limit
    } else {
        limit + COLD_START
    }
}

fn mark_used(state: &AppState, model: &str) {
    lock(&state.llm_used).insert(model.to_owned(), Instant::now());
}

/// Loads the dictation post-processing model while the user is still speaking, so the
/// formatting pass after the recording does not pay the load. Skipped while it is warm.
pub(crate) fn warm_up(app: &AppHandle, settings: &Settings) {
    let Some(model) = post_processing_model(settings).map(str::to_owned) else {
        return;
    };
    let state = app.state::<AppState>();
    let warm = lock(&state.llm_used)
        .get(&model)
        .is_some_and(|at| at.elapsed() < WARM_FOR);
    if warm {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        match state.engine.preload(&model).await {
            Ok(()) => mark_used(&state, &model),
            Err(error) => tracing::info!(%error, model, "language model preload skipped"),
        }
    });
}

/// Sends `prompt` to `model` within `limit` (stretched by [`budget`] for on-device load and
/// generation). The error is a short reason for the user or the log. A timeout drops the
/// request, which cancels it in the helper.
pub(crate) async fn generate(
    state: &AppState,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<Reply, String> {
    generate_with_progress(state, model, prompt, max_tokens, limit, Box::new(|_, _| {})).await
}

/// [`generate`] reporting the helper's progress (0..1, optional message) while it writes.
pub(crate) async fn generate_with_progress(
    state: &AppState,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
    on_progress: ProgressCallback,
) -> Result<Reply, String> {
    let request = GenerateRequest {
        model,
        system: &prompt.system,
        shots: &prompt.shots,
        user: &prompt.user,
        max_tokens: Some(max_tokens),
        // Formatting, edits and meeting answers must follow the text, not improvise.
        temperature: Some(0.0),
    };
    let limit = budget(state, model, limit);
    let reply = state.engine.generate(&request, on_progress);
    match tokio::time::timeout(limit, reply).await {
        Ok(Ok(reply)) => {
            mark_used(state, model);
            Ok(Reply {
                text: reply.text,
                finished: reply.finished,
            })
        }
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err(format!("timed out after {:.1}s", limit.as_secs_f64())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_device_budgets_cover_measured_load_and_generation_times() {
        let short = sv_text::polish_timeout("move the standup to ten tomorrow");
        // Apple Intelligence answered the formatting sample in up to 8.9 s once loaded, and its
        // first request after launch took 34.6 s; the old 2.6 s budget failed both.
        assert!(budget_for(short, true) >= Duration::from_secs(9));
        assert!(budget_for(short, false) >= Duration::from_millis(34_600));
        // A caller's longer budget (a long edit, a meeting summary) is never shortened.
        let long = Duration::from_secs(120);
        assert_eq!(budget_for(long, true), long);
        assert_eq!(budget_for(long, false), long + COLD_START);
    }
}
