// FilePath: crates/sv-cloud/src/lib.rs
//! Remote services: Groq Whisper for transcription and OpenAI-compatible chat completions
//! (Groq, Ollama, LM Studio, any hosted service) for the formatting pass.
#![forbid(unsafe_code)]

pub mod chat;
pub mod groq_whisper;

pub use chat::{chat_url, polish_chat, ChatEndpoint, GROQ_CHAT_URL};
pub use groq_whisper::{groq_transcribe, groq_verify_key, Transcript};

/// The provider's own explanation from an error body. OpenAI and Groq send
/// `{"error": {"message": ...}}`; Ollama sends `{"error": "..."}`.
pub(crate) fn api_error_message(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let error = value.get("error")?;
    let message = error
        .get("message")
        .and_then(|m| m.as_str())
        .or_else(|| error.as_str())?;
    let message = message.trim();
    (!message.is_empty()).then(|| message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_openai_and_ollama_error_shapes() {
        let openai = br#"{"error":{"message":"Invalid API Key","type":"invalid_request_error"}}"#;
        let ollama = br#"{"error":"model not found"}"#;
        assert_eq!(
            api_error_message(ollama).as_deref(),
            Some("model not found")
        );
        assert_eq!(
            api_error_message(openai).as_deref(),
            Some("Invalid API Key")
        );
        assert_eq!(api_error_message(b"<html>bad gateway</html>"), None);
        assert_eq!(api_error_message(br#"{"error":{"code":500}}"#), None);
    }
}
