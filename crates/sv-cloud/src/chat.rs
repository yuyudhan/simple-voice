// FilePath: crates/sv-cloud/src/chat.rs
//! One chat completion over any OpenAI-compatible `/chat/completions` endpoint: Groq by
//! default, or a user-configured one (Ollama, LM Studio, a hosted service). The formatting pass
//! and edit mode share it; each applies its own checks to the reply.

use std::fmt;
use std::time::Duration;

use serde_json::{json, Value};
use sv_text::PolishPrompt;

pub const GROQ_CHAT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

const COMPLETIONS_PATH: &str = "/chat/completions";

pub struct ChatEndpoint<'a> {
    /// Full `/chat/completions` URL; build custom ones with [`chat_url`].
    pub url: String,
    /// Bearer token; local servers usually need none.
    pub key: Option<&'a str>,
    pub model: &'a str,
    /// Sends `reasoning_effort: "none"`, which Groq understands and other servers may reject.
    pub groq_no_reasoning: bool,
}

// The key never reaches logs.
impl fmt::Debug for ChatEndpoint<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatEndpoint")
            .field("url", &self.url)
            .field("key", &self.key.map(|_| "<redacted>"))
            .field("model", &self.model)
            .field("groq_no_reasoning", &self.groq_no_reasoning)
            .finish()
    }
}

/// Completions URL for a user-entered base URL such as `http://localhost:11434/v1`.
pub fn chat_url(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with(COMPLETIONS_PATH) {
        base.to_owned()
    } else {
        format!("{base}{COMPLETIONS_PATH}")
    }
}

/// The model's reply. `finished` is false when it stopped for any reason other than
/// completing its answer (`finish_reason` other than `stop`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatReply {
    pub text: String,
    pub finished: bool,
}

/// Sends `prompt` and returns the reply, or a short reason when the call timed out, failed in
/// transport, returned an HTTP error or an unreadable body.
pub async fn chat_completion(
    client: &reqwest::Client,
    endpoint: &ChatEndpoint<'_>,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<ChatReply, String> {
    let body = chat_body(
        endpoint.model,
        prompt,
        max_tokens,
        endpoint.groq_no_reasoning,
    );
    let mut request = client.post(&endpoint.url).json(&body);
    if let Some(key) = endpoint.key.map(str::trim).filter(|key| !key.is_empty()) {
        request = request.bearer_auth(key);
    }
    let call = async {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await?;
        Ok::<_, reqwest::Error>((status, bytes))
    };

    let reply = match tokio::time::timeout(limit, call).await {
        Err(_) => Err(format!("timed out after {:.1}s", limit.as_secs_f64())),
        Ok(Err(error)) => Err(format!("request failed: {error}")),
        Ok(Ok((status, bytes))) => interpret_response(status, &bytes),
    };
    if let Err(reason) = &reply {
        tracing::info!(url = %endpoint.url, model = endpoint.model, %reason, "chat call failed");
    }
    reply
}

fn chat_body(
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    groq_no_reasoning: bool,
) -> Value {
    let mut messages = Vec::with_capacity(prompt.shots.len() * 2 + 2);
    messages.push(json!({ "role": "system", "content": prompt.system }));
    for (user, assistant) in &prompt.shots {
        messages.push(json!({ "role": "user", "content": user }));
        messages.push(json!({ "role": "assistant", "content": assistant }));
    }
    messages.push(json!({ "role": "user", "content": prompt.user }));

    let mut body = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": max_tokens,
        "messages": messages,
    });
    if groq_no_reasoning {
        if let Some(fields) = body.as_object_mut() {
            fields.insert("reasoning_effort".to_owned(), Value::from("none"));
        }
    }
    body
}

fn interpret_response(status: u16, body: &[u8]) -> Result<ChatReply, String> {
    if status != 200 {
        return Err(crate::api_error_message(body).unwrap_or_else(|| format!("HTTP {status}")));
    }
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Err("unreadable response".to_owned());
    };
    let choice = value.pointer("/choices/0");
    let text = choice
        .and_then(|c| c.pointer("/message/content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let finished = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        == Some("stop");
    Ok(ChatReply { text, finished })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_domain::Style;

    #[test]
    fn chat_url_appends_the_completions_path_once() {
        assert_eq!(
            chat_url("http://localhost:11434/v1/"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            chat_url(" https://api.example.com/v1 "),
            "https://api.example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_url("https://x.dev/v1/chat/completions/"),
            "https://x.dev/v1/chat/completions"
        );
    }

    #[test]
    fn body_has_system_then_shot_pairs_then_the_transcript() {
        let prompt = sv_text::polish_prompt(
            "um so the thing works",
            &[],
            Style::Formal,
            sv_text::PolishTarget::Chat,
        );
        let body = chat_body("qwen/qwen3.8-27b", &prompt, 2048, false);
        assert_eq!(body["model"], "qwen/qwen3.8-27b");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["max_tokens"], 2048);
        assert!(body.get("reasoning_effort").is_none());

        let messages = body["messages"].as_array().cloned().unwrap_or_default();
        let roles: Vec<&str> = messages.iter().filter_map(|m| m["role"].as_str()).collect();
        assert_eq!(
            roles,
            ["system", "user", "assistant", "user", "assistant", "user"]
        );
        assert_eq!(messages[0]["content"], prompt.system.as_str());
        assert_eq!(messages[1]["content"], prompt.shots[0].0.as_str());
        assert_eq!(messages[2]["content"], prompt.shots[0].1.as_str());
        assert_eq!(messages[5]["content"], "um so the thing works");
    }

    #[test]
    fn body_disables_reasoning_only_for_groq() {
        let prompt = sv_text::polish_prompt(
            "one two three",
            &[],
            Style::Casual,
            sv_text::PolishTarget::Chat,
        );
        let body = chat_body("m", &prompt, 2048, true);
        assert_eq!(body["reasoning_effort"], "none");
    }

    fn completion(content: &str, finish_reason: &str) -> Vec<u8> {
        let choice = json!({ "message": { "content": content }, "finish_reason": finish_reason });
        json!({ "choices": [choice] }).to_string().into_bytes()
    }

    #[test]
    fn reply_is_finished_only_when_the_model_stopped_on_its_own() {
        assert_eq!(
            interpret_response(200, &completion(" The build is green. ", "stop")),
            Ok(ChatReply {
                text: " The build is green. ".to_owned(),
                finished: true
            })
        );
        assert_eq!(
            interpret_response(200, &completion("The build", "length")),
            Ok(ChatReply {
                text: "The build".to_owned(),
                finished: false
            })
        );
    }

    #[test]
    fn http_errors_use_the_provider_message_or_the_status() {
        let limited = br#"{"error":{"message":"Rate limit reached"}}"#;
        assert_eq!(
            interpret_response(429, limited),
            Err("Rate limit reached".to_owned())
        );
        assert_eq!(interpret_response(502, b""), Err("HTTP 502".to_owned()));
        assert_eq!(
            interpret_response(200, b"<html>"),
            Err("unreadable response".to_owned())
        );
    }
}
