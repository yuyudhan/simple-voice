// FilePath: crates/sv-engine/src/capture.rs
//! Helper commands that observe the user's surroundings on request: focused-field context,
//! live dictation streams, meeting system audio and speaker diarization (architecture § 6).

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sv_domain::{AppError, AppResult};

use crate::client::EngineClient;
use crate::protocol::{optional_text, params, text, EngineTranscript, QUICK};

/// Loading a streaming recogniser can take a model load on first use.
const STREAM_START: Duration = Duration::from_secs(5 * 60);
const STREAM_FINISH: Duration = Duration::from_secs(120);
const DIARIZE: Duration = Duration::from_secs(2 * 60 * 60);
/// ScreenCaptureKit's first start in a process takes up to ~20 s.
const SYSTEM_AUDIO_START: Duration = Duration::from_secs(60);
const SCREEN_RECORDING_MISSING: &str = "screen recording permission missing";

fn utf8_path(path: &Path) -> AppResult<&str> {
    path.to_str().ok_or_else(|| {
        AppError::invalid(format!(
            "The audio path is not valid UTF-8: {}",
            path.display()
        ))
    })
}

/// Why `focus_context` returned no text. Never an error: the app dictates without context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusUnavailable {
    PermissionDenied,
    ExcludedApp,
    SecureField,
    NoFocusedText,
}

/// A bounded piece of the focused field around the caret, plus where it is.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusContext {
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub window_title: Option<String>,
    /// Accessibility role of the focused element (`AXTextArea`, …).
    pub role: Option<String>,
    pub text: Option<String>,
    /// Caret position inside `text`, in `char`s.
    pub caret: Option<u32>,
    pub reason: Option<FocusUnavailable>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiarizeOptions {
    /// Exact number of speakers, when known.
    pub speakers: Option<u32>,
    pub max_speakers: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerTurn {
    pub start_ms: i64,
    pub end_ms: i64,
    /// `S1`, `S2`, … in order of first appearance.
    pub speaker: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diarization {
    pub segments: Vec<SpeakerTurn>,
    pub speaker_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAudioStart {
    /// Unix milliseconds of the WAV's first sample.
    pub started_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAudioStop {
    pub duration_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StreamStarted {
    stream_id: u64,
}

/// Turns the helper's missing screen-recording error into a permission error.
fn needs_screen_recording(error: AppError) -> AppError {
    match error {
        AppError::Engine(message) if message.contains(SCREEN_RECORDING_MISSING) => {
            AppError::Permission(
                "Screen & System Audio Recording permission is needed to record meeting audio \
                 from other apps. Allow Simple Voice in System Settings → Privacy & Security → \
                 Screen & System Audio Recording."
                    .to_owned(),
            )
        }
        other => other,
    }
}

/// Standard base64 with padding, for the PCM in `stream_audio`.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let sextet = |shift: u32| char::from(ALPHABET[((triple >> shift) & 0x3F) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}

impl EngineClient {
    /// A bounded piece of the focused field around the caret (at most `max_chars`), the window
    /// title and the app. Excluded apps are not read at all; secure fields yield no text.
    pub async fn focus_context(
        &self,
        max_chars: u32,
        excluded_bundle_ids: &[String],
    ) -> AppResult<FocusContext> {
        let excluded = excluded_bundle_ids
            .iter()
            .map(|id| Value::from(id.as_str()))
            .collect();
        let params = params([
            ("maxChars", Some(Value::from(max_chars))),
            ("excludedBundleIds", Some(Value::Array(excluded))),
        ]);
        self.request("focus_context", params, QUICK).await
    }

    /// Starts live recognition on a streaming model and returns the stream id. With `overlay`
    /// the helper shows the hypothesis in the pill while it records. Ends any other stream.
    pub async fn stream_start(
        &self,
        model: &str,
        language: Option<&str>,
        overlay: bool,
    ) -> AppResult<u64> {
        let params = params([
            ("model", text(model)),
            ("language", optional_text(language)),
            ("overlay", Some(Value::from(overlay))),
        ]);
        let result: StreamStarted = self.request("stream_start", params, STREAM_START).await?;
        Ok(result.stream_id)
    }

    /// Queues 16 kHz mono samples for a stream (a notification: no reply, applied in order).
    pub fn stream_audio(&self, stream_id: u64, pcm: &[i16]) -> AppResult<()> {
        let mut bytes = Vec::with_capacity(pcm.len() * 2);
        for sample in pcm {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.notify(
            "stream_audio",
            params([
                ("streamId", Some(Value::from(stream_id))),
                ("pcm", Some(Value::from(base64(&bytes)))),
            ]),
        )
    }

    /// Flushes the stream and returns the final transcript; dropping the future cancels it.
    pub async fn stream_finish(&self, stream_id: u64) -> AppResult<EngineTranscript> {
        let params = params([("streamId", Some(Value::from(stream_id)))]);
        self.request_cancellable("stream_finish", params, None, STREAM_FINISH)
            .await
    }

    /// Ends a stream without a result; unknown ids are fine.
    pub async fn stream_cancel(&self, stream_id: u64) -> AppResult<()> {
        let params = params([("streamId", Some(Value::from(stream_id)))]);
        self.command("stream_cancel", params, QUICK).await
    }

    /// Who spoke when, on the downloaded `speaker-diarization` model. Dropping the future
    /// cancels it.
    pub async fn diarize(
        &self,
        wav_path: &Path,
        options: DiarizeOptions,
    ) -> AppResult<Diarization> {
        let path = utf8_path(wav_path)?;
        let params = params([
            ("wavPath", text(path)),
            ("speakers", options.speakers.map(Value::from)),
            ("maxSpeakers", options.max_speakers.map(Value::from)),
        ]);
        self.request_cancellable("diarize", params, None, DIARIZE)
            .await
    }

    /// Starts writing everything the Mac plays to `wav_path` (16 kHz mono PCM16).
    pub async fn system_audio_start(&self, wav_path: &Path) -> AppResult<SystemAudioStart> {
        let path = utf8_path(wav_path)?;
        let params = params([("wavPath", text(path))]);
        self.request("system_audio_start", params, SYSTEM_AUDIO_START)
            .await
            .map_err(needs_screen_recording)
    }

    /// Stops the system-audio capture and finalizes its WAV.
    pub async fn system_audio_stop(&self) -> AppResult<SystemAudioStop> {
        self.request("system_audio_stop", Value::Null, QUICK).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use super::*;
    use crate::client::Transport;

    /// Answers every request with a canned reply and records every line sent.
    struct Scripted {
        client: EngineClient,
        reply: String,
        seen: Mutex<Vec<Value>>,
    }

    impl Transport for Scripted {
        fn send_line(&self, line: String) -> AppResult<()> {
            let request: Value = serde_json::from_str(&line).unwrap();
            let id = request["id"].as_u64();
            self.seen.lock().push(request);
            // Notifications carry no id and get no reply.
            let Some(id) = id else { return Ok(()) };
            let client = self.client.clone();
            let reply = self.reply.replace("$ID", &id.to_string());
            tokio::spawn(async move { client.on_stdout_line(&reply) });
            Ok(())
        }
    }

    fn scripted(reply: &str) -> (EngineClient, Arc<Scripted>) {
        let client = EngineClient::new();
        let transport = Arc::new(Scripted {
            client: client.clone(),
            reply: reply.to_owned(),
            seen: Mutex::new(Vec::new()),
        });
        client.connect(transport.clone());
        (client, transport)
    }

    #[tokio::test]
    async fn focus_context_reads_reasons() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"appName":"1Password","bundleId":"com.1password",
                "role":"AXSecureTextField","reason":"secure_field"}}"#,
        );
        let excluded = vec!["com.apple.Terminal".to_owned()];
        let context = client.focus_context(800, &excluded).await.unwrap();
        assert_eq!(context.reason, Some(FocusUnavailable::SecureField));
        assert_eq!(context.text, None);
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["maxChars"], 800);
        assert_eq!(
            request["excludedBundleIds"],
            serde_json::json!(["com.apple.Terminal"])
        );
    }

    #[tokio::test]
    async fn diarize_sends_counts_and_reads_turns() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"segments":[
                {"startMs":0,"endMs":1500,"speaker":"S1"},
                {"startMs":1500,"endMs":4000,"speaker":"S2"}],"speakerCount":2}}"#,
        );
        let options = DiarizeOptions {
            speakers: None,
            max_speakers: Some(4),
        };
        let result = client
            .diarize(Path::new("/tmp/meeting.wav"), options)
            .await
            .unwrap();
        assert_eq!(result.speaker_count, 2);
        assert_eq!(result.segments[1].speaker, "S2");
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["maxSpeakers"], 4);
        assert!(request.get("speakers").is_none());
    }

    #[tokio::test]
    async fn system_audio_without_permission_is_a_permission_error() {
        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":false,"error":"screen recording permission missing"}"#);
        let result = client
            .system_audio_start(Path::new("/tmp/system.wav"))
            .await;
        assert!(matches!(result, Err(AppError::Permission(_))));
    }

    #[tokio::test]
    async fn streams_start_feed_and_finish() {
        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"streamId":7}}"#);
        let id = client
            .stream_start("parakeet-flash", Some("en"), true)
            .await
            .unwrap();
        assert_eq!(id, 7);
        client.stream_audio(id, &[0, 1, -1, i16::MAX]).unwrap();
        let seen = transport.seen.lock().clone();
        assert_eq!(seen[0]["overlay"], true);
        assert_eq!(seen[1]["cmd"], "stream_audio");
        assert!(seen[1].get("id").is_none());
        assert_eq!(seen[1]["streamId"], 7);
        // 00 00 01 00 FF FF FF 7F, little endian.
        assert_eq!(seen[1]["pcm"], "AAABAP///38=");
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"hi"), "aGk=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFB, 0xFF]), "+/8=");
    }
}
