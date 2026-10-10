// FilePath: mobile/plugins/sv-native/src/request.rs
//! What Swift asks Rust for. Field names match the Swift encoder (camelCase).

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NativeRequest {
    /// The keyboard status line for this reach; answered with the status and session length.
    #[serde(rename_all = "camelCase")]
    Status {
        online: bool,
        on_device_speech: bool,
    },
    /// Transcribe a finished recording with Groq; answered with a `Dictation`.
    #[serde(rename_all = "camelCase")]
    Dictate { wav_path: String, duration_ms: i64 },
    /// Apple Speech already transcribed the recording offline; answered with a `Dictation`.
    #[serde(rename_all = "camelCase")]
    Local {
        wav_path: String,
        duration_ms: i64,
        text: String,
        language: Option<String>,
        transcribe_ms: i64,
    },
    /// Re-run a failed dictation from its kept audio; answered with a `Dictation`.
    #[serde(rename_all = "camelCase")]
    Retry { history_id: i64 },
    /// The text went to the clipboard because no keyboard confirmed inserting it (D-15).
    #[serde(rename_all = "camelCase")]
    NotInserted { history_id: i64 },
}

#[cfg(any(target_os = "ios", test))]
#[derive(Deserialize)]
struct Envelope {
    id: u64,
    #[serde(flatten)]
    request: NativeRequest,
}

#[cfg(target_os = "ios")]
pub(crate) fn dispatch<F: Fn(u64, NativeRequest)>(
    body: tauri::ipc::InvokeResponseBody,
    on_request: &F,
) {
    let parsed = match body {
        tauri::ipc::InvokeResponseBody::Json(json) => serde_json::from_str::<Envelope>(&json),
        tauri::ipc::InvokeResponseBody::Raw(bytes) => serde_json::from_slice::<Envelope>(&bytes),
    };
    match parsed {
        Ok(envelope) => on_request(envelope.id, envelope.request),
        Err(error) => tracing::error!(%error, "unreadable native request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swift_requests_decode_with_their_id() {
        let json = r#"{"id":7,"kind":"local","wavPath":"/a.wav","durationMs":900,
            "text":"hi","language":null,"transcribeMs":40}"#;
        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.id, 7);
        assert_eq!(
            envelope.request,
            NativeRequest::Local {
                wav_path: "/a.wav".to_owned(),
                duration_ms: 900,
                text: "hi".to_owned(),
                language: None,
                transcribe_ms: 40,
            }
        );
    }
}
