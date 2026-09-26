// FilePath: src-tauri/src/features/dictation/llm.rs
//! One request to the configured post-processing provider: Groq, a custom OpenAI-compatible
//! endpoint, or Apple Intelligence through the engine helper. The formatting pass and edit mode
//! both call it and judge the reply with their own checks.

use std::time::Duration;

use sv_cloud::{ChatEndpoint, ChatReply};
use sv_domain::models::APPLE_INTELLIGENCE;
use sv_domain::{PostProcessing, Settings};
use sv_text::{PolishPrompt, PolishTarget};

use super::transcription::groq_key;
use crate::state::AppState;

/// The prompt family the configured provider needs.
pub(super) fn target(settings: &Settings) -> PolishTarget {
    match settings.post_processing {
        PostProcessing::Apple => PolishTarget::OnDevice,
        PostProcessing::Off | PostProcessing::Groq | PostProcessing::Custom => PolishTarget::Chat,
    }
}

/// The model id a successful request ran on, as recorded in history.
pub(super) fn model_id(settings: &Settings) -> Option<String> {
    match settings.post_processing {
        PostProcessing::Off => None,
        PostProcessing::Groq => Some(settings.groq_formatting_model.clone()),
        PostProcessing::Custom => Some(settings.custom_model.trim().to_owned()),
        PostProcessing::Apple => Some(APPLE_INTELLIGENCE.to_owned()),
    }
}

/// Sends `prompt` within `limit`. The error is a short reason for the user or the log.
pub(super) async fn complete(
    state: &AppState,
    settings: &Settings,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<ChatReply, String> {
    match settings.post_processing {
        PostProcessing::Off => Err("AI post-processing is off".to_owned()),
        PostProcessing::Groq => {
            let key = groq_key(state)
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "no Groq API key".to_owned())?;
            let endpoint = ChatEndpoint {
                url: sv_cloud::GROQ_CHAT_URL.to_owned(),
                key: Some(&key),
                model: &settings.groq_formatting_model,
                groq_no_reasoning: true,
            };
            sv_cloud::chat_completion(&state.http, &endpoint, prompt, max_tokens, limit).await
        }
        PostProcessing::Custom => {
            if settings.custom_model.trim().is_empty() {
                return Err("no custom model configured".to_owned());
            }
            let key = match state.db() {
                Ok(db) => db.custom_api_key().await.ok().flatten(),
                Err(_) => None,
            };
            let endpoint = ChatEndpoint {
                url: sv_cloud::chat_url(&settings.custom_base_url),
                key: key.as_deref().filter(|key| !key.trim().is_empty()),
                model: &settings.custom_model,
                groq_no_reasoning: false,
            };
            sv_cloud::chat_completion(&state.http, &endpoint, prompt, max_tokens, limit).await
        }
        PostProcessing::Apple => {
            let request = state
                .engine
                .polish(&prompt.system, &prompt.shots, &prompt.user);
            match tokio::time::timeout(limit, request).await {
                Ok(Ok(reply)) => Ok(ChatReply {
                    text: reply.text,
                    finished: reply.finished,
                }),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err(format!("timed out after {:.1}s", limit.as_secs_f64())),
            }
        }
    }
}
