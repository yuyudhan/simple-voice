// FilePath: src-tauri/src/features/llm.rs
//! One request to a language model: Apple Intelligence or the downloaded local model through
//! the engine helper, or, when the user chose it, a Groq chat model with their own key. Dictation
//! formatting, edit mode, learning and meetings all call it and judge the reply with their own
//! checks. Only a `groq:` model id reaches the network, and a Groq request that fails or runs
//! late falls back to an on-device model that is ready.

use std::time::{Duration, Instant};

use sv_domain::models::{groq_chat_model, groq_llm_id, APPLE_INTELLIGENCE};
use sv_domain::{LlmBackend, ModelStatus, PostProcessing, Settings};
use sv_engine::{GenerateRequest, ProgressCallback};
use sv_text::{PolishPrompt, PolishTarget};

use tauri::{AppHandle, Manager};

use crate::features::groq;
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

/// A reply and the id of the model that wrote it (the on-device fallback after a failed Groq
/// request), as history records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Answer {
    pub(crate) reply: Reply,
    pub(crate) model: String,
}

/// The model dictation post-processing runs on; `None` when it is off.
pub(crate) fn post_processing_model(settings: &Settings) -> Option<String> {
    match settings.post_processing {
        PostProcessing::Off => None,
        PostProcessing::Apple => Some(APPLE_INTELLIGENCE.to_owned()),
        PostProcessing::Local => Some(settings.local_llm_model.clone()),
        PostProcessing::Groq => Some(groq_llm_id(&settings.groq_formatting_model)),
    }
}

/// The model a backend choice runs on.
pub(crate) fn backend_model(settings: &Settings, backend: LlmBackend) -> String {
    match backend {
        LlmBackend::Apple => APPLE_INTELLIGENCE.to_owned(),
        LlmBackend::Local => settings.local_llm_model.clone(),
        LlmBackend::Groq => groq_llm_id(&settings.groq_formatting_model),
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

/// Why `model` cannot answer right now, for the user; `None` when it is ready. A Groq model is
/// ready once a key is saved.
pub(crate) async fn unavailable(
    state: &AppState,
    settings: &Settings,
    model: &str,
) -> Option<String> {
    if groq_chat_model(model).is_some() {
        return match groq::key(state).await {
            Ok(Some(_)) => None,
            Ok(None) => Some(groq::NO_KEY.to_owned()),
            Err(error) => Some(error.to_string()),
        };
    }
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

/// Sends the prompt `build` makes for the model's family to the dictation post-processing model
/// (formatting, edit mode, learning). A failed Groq request is retried once on the first ready
/// on-device model, with the prompt rebuilt for its family.
pub(crate) async fn post_process(
    state: &AppState,
    settings: &Settings,
    build: impl Fn(PolishTarget) -> PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<Answer, String> {
    let Some(model) = post_processing_model(settings) else {
        return Err("AI post-processing is off".to_owned());
    };
    let reason = match generate(state, &model, &build(target(&model)), max_tokens, limit).await {
        Ok(reply) => return Ok(Answer { reply, model }),
        Err(reason) if groq_chat_model(&model).is_some() => reason,
        Err(reason) => return Err(reason),
    };
    let Some(fallback) = on_device_fallback(state, settings, None).await else {
        return Err(reason);
    };
    tracing::info!(%reason, fallback, "groq failed; post-processing on the Mac");
    let prompt = build(target(fallback));
    match generate(state, fallback, &prompt, max_tokens, limit).await {
        Ok(reply) => Ok(Answer {
            reply,
            model: fallback.to_owned(),
        }),
        Err(error) => Err(format!(
            "{reason}; the on-device fallback failed too: {error}"
        )),
    }
}

/// Sends `prompt` to `model` like [`generate_with_progress`]; a failed Groq request is retried
/// once on an on-device model of the same prompt family (the local model), since `prompt` was
/// built for that family.
pub(crate) async fn generate_with_fallback(
    state: &AppState,
    settings: &Settings,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
    on_progress: ProgressCallback,
) -> Result<Answer, String> {
    if groq_chat_model(model).is_none() {
        let reply =
            generate_with_progress(state, model, prompt, max_tokens, limit, on_progress).await?;
        return Ok(Answer {
            reply,
            model: model.to_owned(),
        });
    }
    let reason = match generate(state, model, prompt, max_tokens, limit).await {
        Ok(reply) => {
            return Ok(Answer {
                reply,
                model: model.to_owned(),
            })
        }
        Err(reason) => reason,
    };
    let Some(fallback) = on_device_fallback(state, settings, Some(target(model))).await else {
        return Err(reason);
    };
    tracing::info!(%reason, fallback, "groq failed; answering on the Mac");
    match generate_with_progress(state, fallback, prompt, max_tokens, limit, on_progress).await {
        Ok(reply) => Ok(Answer {
            reply,
            model: fallback.to_owned(),
        }),
        Err(error) => Err(format!(
            "{reason}; the on-device fallback failed too: {error}"
        )),
    }
}

/// The on-device model a failed Groq request falls back to: the local language model, else
/// Apple Intelligence, whichever is ready first and (when `family` is given) takes that prompt
/// family.
async fn on_device_fallback<'a>(
    state: &AppState,
    settings: &'a Settings,
    family: Option<PolishTarget>,
) -> Option<&'a str> {
    for model in [settings.local_llm_model.as_str(), APPLE_INTELLIGENCE] {
        if family.is_some_and(|family| family != target(model)) {
            continue;
        }
        if unavailable(state, settings, model).await.is_none() {
            return Some(model);
        }
    }
    None
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
    // A Groq model has nothing to load.
    let Some(model) = post_processing_model(settings).filter(|m| groq_chat_model(m).is_none())
    else {
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
/// generation; a Groq model gets `limit` as is). The error is a short reason for the user or the
/// log. A timeout drops the request, which cancels it in the helper.
pub(crate) async fn generate(
    state: &AppState,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<Reply, String> {
    generate_with_progress(state, model, prompt, max_tokens, limit, Box::new(|_, _| {})).await
}

/// [`generate`] reporting the helper's progress (0..1, optional message) while it writes. A
/// Groq model answers in one piece and reports none.
pub(crate) async fn generate_with_progress(
    state: &AppState,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
    on_progress: ProgressCallback,
) -> Result<Reply, String> {
    if let Some(groq_model) = groq_chat_model(model) {
        return groq::chat(state, groq_model, prompt, max_tokens, limit).await;
    }
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

    #[test]
    fn groq_choices_name_a_groq_model_with_the_hosted_prompt_family() {
        let settings = Settings {
            post_processing: PostProcessing::Groq,
            groq_formatting_model: "llama-3.3-70b-versatile".to_owned(),
            ..Settings::default()
        };
        let model = post_processing_model(&settings);
        assert_eq!(model.as_deref(), Some("groq:llama-3.3-70b-versatile"));
        assert_eq!(
            model.as_deref().and_then(groq_chat_model),
            Some("llama-3.3-70b-versatile")
        );
        assert_eq!(model.as_deref().map(target), Some(PolishTarget::Chat));
        assert_eq!(
            backend_model(&settings, LlmBackend::Groq),
            "groq:llama-3.3-70b-versatile"
        );
        // On-device ids never reach Groq.
        assert_eq!(groq_chat_model(&settings.local_llm_model), None);
        assert_eq!(groq_chat_model(APPLE_INTELLIGENCE), None);
        assert_eq!(
            post_processing_model(&Settings::default()).as_deref(),
            Some(APPLE_INTELLIGENCE)
        );
    }
}
