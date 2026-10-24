// FilePath: crates/sv-cloud/src/lib.rs
//! The optional Groq provider, used only when the user picks it and saves their own API key:
//! Groq Whisper for dictation transcription and Groq chat completions for the formatting pass,
//! edit mode, learning and the meeting assistant. Nothing here runs for the default on-device
//! providers, and nothing is sent anywhere but Groq's API.
#![forbid(unsafe_code)]

pub mod chat;
pub mod groq_whisper;

pub use chat::{chat_completion, ChatReply, GroqChat};
pub use groq_whisper::{groq_transcribe, groq_verify_key, upload_timeout, GroqWhisper, Transcript};

/// The provider's own explanation from an error body: Groq sends
/// `{"error": {"message": ...}}`, some proxies `{"error": "..."}`.
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
    fn reads_both_error_shapes() {
        let groq = br#"{"error":{"message":"Invalid API Key","type":"invalid_request_error"}}"#;
        let flat = br#"{"error":"model not found"}"#;
        assert_eq!(api_error_message(flat).as_deref(), Some("model not found"));
        assert_eq!(api_error_message(groq).as_deref(), Some("Invalid API Key"));
        assert_eq!(api_error_message(b"<html>bad gateway</html>"), None);
        assert_eq!(api_error_message(br#"{"error":{"code":500}}"#), None);
    }
}
