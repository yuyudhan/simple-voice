// FilePath: crates/sv-cloud/src/chat.rs
//! One Groq chat completion (`/openai/v1/chat/completions`). The formatting pass, edit mode,
//! learning and the meeting assistant share it; each applies its own checks to the reply.

use std::fmt;
use std::time::Duration;

use serde_json::{json, Value};
use sv_text::PolishPrompt;

const GROQ_CHAT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";

/// Which Groq chat model answers, with the user's key.
pub struct GroqChat<'a> {
    pub key: &'a str,
    /// A Groq model id such as `qwen/qwen3.8-27b`.
    pub model: &'a str,
}

// The key never reaches logs.
impl fmt::Debug for GroqChat<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroqChat")
            .field("key", &"<redacted>")
            .field("model", &self.model)
            .finish()
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
    chat: &GroqChat<'_>,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<ChatReply, String> {
    let body = chat_body(chat.model, prompt, max_tokens);
    let request = client
        .post(GROQ_CHAT_URL)
        .bearer_auth(chat.key.trim())
        .json(&body);
    let call = async {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let bytes = response.bytes().await?;
        Ok::<_, reqwest::Error>((status, bytes))
    };

    let reply = match tokio::time::timeout(limit, call).await {
        Err(_) => Err(format!("Groq timed out after {:.1}s", limit.as_secs_f64())),
        Ok(Err(error)) => Err(format!("Groq request failed: {error}")),
        Ok(Ok((status, bytes))) => interpret_response(status, &bytes),
    };
    if let Err(reason) = &reply {
        tracing::info!(model = chat.model, %reason, "groq chat call failed");
    }
    reply
}

/// Qwen models on Groq think before answering unless told not to; formatting and meeting
/// answers must come back fast and plain. Other Groq models reject the field.
fn disables_reasoning(model: &str) -> bool {
    model.starts_with("qwen/")
}

fn chat_body(model: &str, prompt: &PolishPrompt, max_tokens: u32) -> Value {
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
    if disables_reasoning(model) {
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
    fn body_has_system_then_shot_pairs_then_the_transcript() {
        let prompt = sv_text::polish_prompt(
            "um so the thing works",
            &[],
            Style::Formal,
            sv_text::PolishTarget::Chat,
            &["en".to_owned(), "hi-Latn".to_owned()],
        );
        let body = chat_body("llama-3.3-70b-versatile", &prompt, 2048);
        assert_eq!(body["model"], "llama-3.3-70b-versatile");
        assert_eq!(body["temperature"], 0);
        assert_eq!(body["max_tokens"], 2048);
        assert!(body.get("reasoning_effort").is_none());

        let messages = body["messages"].as_array().cloned().unwrap_or_default();
        let roles: Vec<&str> = messages.iter().filter_map(|m| m["role"].as_str()).collect();
        let mut expected = vec!["system"];
        expected.extend(prompt.shots.iter().flat_map(|_| ["user", "assistant"]));
        expected.push("user");
        assert_eq!(roles, expected);
        assert_eq!(messages[0]["content"], prompt.system.as_str());
        assert_eq!(messages[1]["content"], prompt.shots[0].0.as_str());
        assert_eq!(messages[2]["content"], prompt.shots[0].1.as_str());
        assert_eq!(
            messages[messages.len() - 1]["content"],
            prompt.user.as_str()
        );
    }

    #[test]
    fn body_disables_reasoning_only_for_qwen() {
        let prompt = sv_text::polish_prompt(
            "one two three",
            &[],
            Style::Casual,
            sv_text::PolishTarget::Chat,
            &["en".to_owned()],
        );
        let body = chat_body("qwen/qwen3.8-27b", &prompt, 2048);
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
