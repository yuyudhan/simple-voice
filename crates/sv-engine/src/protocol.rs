// FilePath: crates/sv-engine/src/protocol.rs
//! Typed wrappers for every helper command (architecture § 6).
//!
//! Timeouts are generous upper bounds that only guard against a wedged helper; callers that have
//! a tighter budget (the polish pass, for one) race these futures against their own deadline.

use std::path::Path;
use std::time::Duration;

use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sv_domain::{
    AppError, AppResult, DictationState, ModelStatus, PermissionKind, Permissions,
    TranscriptQuality,
};

use crate::client::{EngineClient, ProgressCallback};

pub(crate) const QUICK: Duration = Duration::from_secs(10);
const MODEL_STATUS: Duration = Duration::from_secs(30);
const DOWNLOAD: Duration = Duration::from_secs(2 * 60 * 60);
const PRELOAD: Duration = Duration::from_secs(5 * 60);
const TRANSCRIBE: Duration = Duration::from_secs(120);
/// Timed transcription is for meetings, which can run for hours.
const TRANSCRIBE_LONG: Duration = Duration::from_secs(2 * 60 * 60);
/// Local generation of a long summary on a 4B model takes minutes, including the model load.
const GENERATE: Duration = Duration::from_secs(15 * 60);
/// The user may leave the system prompt open for a while before answering it.
const REQUEST_PERMISSION: Duration = Duration::from_secs(10 * 60);
/// Apple Intelligence answers in seconds; a first local-model polish also loads the model.
const POLISH: Duration = Duration::from_secs(120);
/// The helper answers `watch_edits` when its watch window closes; this covers the rest.
const EDIT_WATCH_SLACK: Duration = Duration::from_secs(10);

const ACCESSIBILITY_MISSING: &str = "accessibility permission missing";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatusResult {
    pub status: ModelStatus,
    /// Bytes on disk when ready; the download size when not downloaded and the catalog pins it.
    pub size_bytes: Option<u64>,
    pub reason: Option<String>,
    /// Whether `stream_start` accepts the model (true incremental recognition).
    #[serde(default)]
    pub streaming: bool,
    /// Prompt plus answer token budget of a text model.
    #[serde(default)]
    pub context_tokens: Option<u32>,
    #[serde(default)]
    pub license: Option<String>,
    /// Provenance: the repository (and upstream model) the files come from.
    #[serde(default)]
    pub source: Option<String>,
    /// Pinned repository commit, when the catalog pins one.
    #[serde(default)]
    pub revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTranscript {
    pub text: String,
    pub language: Option<String>,
    /// Absent for models that report nothing (Parakeet Flash, Apple Speech).
    #[serde(default)]
    pub quality: TranscriptQuality,
    /// Timed segments, present when the request asked for `timestamps`.
    #[serde(default)]
    pub segments: Vec<TimedSegment>,
    /// Dictionary terms Parakeet's booster wrote in from the audio (`vocabulary` requests).
    #[serde(default)]
    pub dictionary_fixes: u32,
}

/// A stretch of transcript and its place in the audio, in milliseconds from the start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// How a `transcribe` call should recognise the recording.
#[derive(Debug, Clone, Copy, Default)]
pub struct TranscribeOptions<'a> {
    /// Pins recognition to this ISO 639-1 language.
    pub language: Option<&'a str>,
    /// Allowed ISO 639-1 languages; Whisper models detect among these. Empty = any.
    pub languages: &'a [String],
    /// Whisper conditioning prompt (`sv_text::whisper_prompt`); empty = none.
    pub prompt: &'a str,
    /// Personal dictionary names Parakeet TDT rescores its transcript against, once the
    /// `parakeet-vocabulary` model is downloaded (`Vocabulary::boost_terms`); empty = none.
    pub vocabulary: &'a [String],
    /// Return timed `segments` (meetings); also allows hours of audio instead of a dictation.
    pub timestamps: bool,
}

/// A text-generation request (summaries, transcript chat, translation).
#[derive(Debug, Clone, Copy)]
pub struct GenerateRequest<'a> {
    /// `apple-intelligence` or a downloaded local model (`qwen3-4b-instruct`).
    pub model: &'a str,
    pub system: &'a str,
    /// `(user, assistant)` example pairs.
    pub shots: &'a [(String, String)],
    pub user: &'a str,
    /// Answer budget; the helper defaults to 1024.
    pub max_tokens: Option<u32>,
    /// The helper defaults to 0.2; 0 is deterministic.
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontmostApp {
    pub name: Option<String>,
    pub bundle_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolishReply {
    pub text: String,
    /// False when generation stopped early (context or guardrail limits); the caller should
    /// treat the text as truncated.
    pub finished: bool,
}

/// The pasted span as it read when the helper stopped watching it. `text` is absent when the
/// span could not be read, and `reason` then says why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditWatch {
    pub text: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SelectedText {
    text: Option<String>,
}

/// Turns the helper's missing-trust error into a permission error that names what needs it.
fn needs_accessibility(error: AppError, action: &str) -> AppError {
    match error {
        AppError::Engine(message) if message.contains(ACCESSIBILITY_MISSING) => {
            AppError::Permission(format!(
                "Accessibility permission is needed to {action}. Allow Simple Voice in System \
                 Settings → Privacy & Security → Accessibility."
            ))
        }
        other => other,
    }
}

/// Where Simple Voice stands as a login item (`SMAppService.mainApp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginItemStatus {
    Enabled,
    Disabled,
    /// Registered but switched off in System Settings; only the user can allow it again there.
    RequiresApproval,
}

#[derive(Debug, Deserialize)]
struct DownloadResult {
    status: ModelStatus,
}

#[derive(Debug, Deserialize)]
struct MutedResult {
    previous: bool,
}

#[derive(Debug, Deserialize)]
struct FnKeyWatch {
    active: bool,
}

#[derive(Debug, Deserialize)]
struct LoginItem {
    status: LoginItemStatus,
}

fn shot_values(shots: &[(String, String)]) -> Value {
    Value::Array(
        shots
            .iter()
            .map(|(example_user, example_assistant)| {
                params([
                    ("user", text(example_user)),
                    ("assistant", text(example_assistant)),
                ])
            })
            .collect(),
    )
}

/// Builds a flat parameter object, leaving out absent optional values.
pub(crate) fn params<const N: usize>(pairs: [(&str, Option<Value>); N]) -> Value {
    let map: Map<String, Value> = pairs
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    Value::Object(map)
}

pub(crate) fn text(value: &str) -> Option<Value> {
    Some(Value::from(value))
}

pub(crate) fn optional_text(value: Option<&str>) -> Option<Value> {
    value.map(Value::from)
}

impl EngineClient {
    pub async fn ping(&self) -> AppResult<PingResult> {
        self.request("ping", Value::Null, QUICK).await
    }

    /// `language` only matters for `apple-speech`, whose assets are per locale.
    pub async fn model_status(
        &self,
        model: &str,
        language: Option<&str>,
    ) -> AppResult<ModelStatusResult> {
        let params = params([
            ("model", text(model)),
            ("language", optional_text(language)),
        ]);
        self.request("model_status", params, MODEL_STATUS).await
    }

    pub async fn download_model(
        &self,
        model: &str,
        language: Option<&str>,
        on_progress: ProgressCallback,
    ) -> AppResult<()> {
        let params = params([
            ("model", text(model)),
            ("language", optional_text(language)),
        ]);
        let result: DownloadResult = self
            .request_cancellable("download_model", params, Some(on_progress), DOWNLOAD)
            .await?;
        match result.status {
            ModelStatus::Ready => Ok(()),
            other => Err(AppError::Engine(format!(
                "The model download for {model} ended in state {other:?} instead of ready"
            ))),
        }
    }

    pub async fn delete_model(&self, model: &str) -> AppResult<()> {
        self.command(
            "delete_model",
            params([("model", text(model))]),
            MODEL_STATUS,
        )
        .await
    }

    pub async fn preload(&self, model: &str) -> AppResult<()> {
        self.command("preload", params([("model", text(model))]), PRELOAD)
            .await
    }

    /// `wav_path` must be 16 kHz mono PCM16. Empty `languages`, `prompt` and `vocabulary` are
    /// left out of the request; Whisper reads the first two, Parakeet TDT the last.
    pub async fn transcribe(
        &self,
        model: &str,
        wav_path: &Path,
        options: TranscribeOptions<'_>,
    ) -> AppResult<EngineTranscript> {
        let path = wav_path.to_str().ok_or_else(|| {
            AppError::invalid(format!(
                "The recording path is not valid UTF-8: {}",
                wav_path.display()
            ))
        })?;
        let prompt = (!options.prompt.is_empty()).then(|| Value::from(options.prompt));
        let params = params([
            ("model", text(model)),
            ("wavPath", text(path)),
            ("language", optional_text(options.language)),
            ("languages", Self::strings(options.languages)),
            ("prompt", prompt),
            ("vocabulary", Self::strings(options.vocabulary)),
            (
                "timestamps",
                options.timestamps.then_some(Value::from(true)),
            ),
        ]);
        let timeout = if options.timestamps {
            TRANSCRIBE_LONG
        } else {
            TRANSCRIBE
        };
        self.request_cancellable("transcribe", params, None, timeout)
            .await
    }

    /// A JSON array of `values`, or `None` (left out of the request) when there are none.
    fn strings(values: &[String]) -> Option<Value> {
        (!values.is_empty())
            .then(|| Value::Array(values.iter().map(|v| Value::from(v.as_str())).collect()))
    }

    pub async fn permissions(&self) -> AppResult<Permissions> {
        self.request("permissions", Value::Null, QUICK).await
    }

    /// Shows the system prompt where macOS allows one and returns the statuses afterwards.
    pub async fn request_permission(&self, kind: PermissionKind) -> AppResult<Permissions> {
        let params = params([("kind", text(kind.as_str()))]);
        self.request("request_permission", params, REQUEST_PERMISSION)
            .await
    }

    /// Opens the matching Privacy & Security pane in System Settings.
    pub async fn open_settings(&self, kind: PermissionKind) -> AppResult<()> {
        let params = params([("kind", text(kind.as_str()))]);
        self.command("open_settings", params, QUICK).await
    }

    pub async fn frontmost_app(&self) -> AppResult<FrontmostApp> {
        self.request("frontmost_app", Value::Null, QUICK).await
    }

    /// Posts Cmd+V to the focused app.
    pub async fn paste(&self) -> AppResult<()> {
        self.command("paste", Value::Null, QUICK)
            .await
            .map_err(|error| needs_accessibility(error, "paste"))
    }

    /// The text selected in the focused app; `None` when nothing is selected. The helper may
    /// copy the selection with Cmd+C to read it and restores the clipboard afterwards.
    pub async fn selected_text(&self) -> AppResult<Option<String>> {
        let result: SelectedText = self
            .request("selected_text", Value::Null, QUICK)
            .await
            .map_err(|error| needs_accessibility(error, "read the selected text"))?;
        Ok(result.text)
    }

    /// Enables the frontmost app's accessibility tree ahead of a paste, so a web view has
    /// built it by the time `watch_edits` reads the field. Supersedes a running watch.
    pub async fn prepare_edit_watch(&self) -> AppResult<()> {
        self.command("prepare_edit_watch", Value::Null, QUICK)
            .await
            .map_err(|error| needs_accessibility(error, "watch for corrections"))
    }

    /// Follows the just-pasted `text` in the focused field for up to `window` and returns how it
    /// reads when the watch ends (focus moved, field cleared, window over or superseded).
    pub async fn watch_edits(&self, text: &str, window: Duration) -> AppResult<EditWatch> {
        let timeout_ms = u64::try_from(window.as_millis()).unwrap_or(u64::MAX);
        let params = params([
            ("text", Some(Value::from(text))),
            ("timeoutMs", Some(Value::from(timeout_ms))),
        ]);
        self.request(
            "watch_edits",
            params,
            window.saturating_add(EDIT_WATCH_SLACK),
        )
        .await
        .map_err(|error| needs_accessibility(error, "watch for corrections"))
    }

    /// Mutes or restores system output; returns whether output was muted before the call.
    pub async fn set_output_muted(&self, muted: bool) -> AppResult<bool> {
        let params = params([("muted", Some(Value::from(muted)))]);
        let result: MutedResult = self.request("set_output_muted", params, QUICK).await?;
        Ok(result.previous)
    }

    /// Starts or stops the Fn key events. Returns whether the key is being watched now; while
    /// Accessibility access is missing the helper keeps retrying and reports `false`.
    pub async fn watch_fn_key(&self, enabled: bool) -> AppResult<bool> {
        let params = params([("enabled", Some(Value::from(enabled)))]);
        let result: FnKeyWatch = self.request("watch_fn_key", params, QUICK).await?;
        Ok(result.active)
    }

    pub async fn login_item(&self) -> AppResult<LoginItemStatus> {
        let result: LoginItem = self.request("login_item", Value::Null, QUICK).await?;
        Ok(result.status)
    }

    /// Registers or removes the app as a login item and returns the status afterwards. When
    /// macOS holds a registration for approval the helper opens the Login Items pane.
    pub async fn set_login_item(&self, enabled: bool) -> AppResult<LoginItemStatus> {
        let params = params([("enabled", Some(Value::from(enabled)))]);
        let result: LoginItem = self.request("set_login_item", params, QUICK).await?;
        Ok(result.status)
    }

    /// Runs the post-processing prompt on `model` (`None`: Apple Intelligence; or a downloaded
    /// local model such as `qwen3-4b-instruct`). `shots` are `(user, assistant)` example pairs.
    /// Dropping the future cancels the generation.
    pub async fn polish(
        &self,
        model: Option<&str>,
        system: &str,
        shots: &[(String, String)],
        user: &str,
    ) -> AppResult<PolishReply> {
        let params = params([
            ("model", optional_text(model)),
            ("system", text(system)),
            ("shots", Some(shot_values(shots))),
            ("user", text(user)),
        ]);
        self.request_cancellable("polish", params, None, POLISH)
            .await
    }

    /// Summaries, transcript chat and translation on a local text model. Progress reports the
    /// share of the answer budget used; dropping the future cancels the generation.
    pub async fn generate(
        &self,
        request: &GenerateRequest<'_>,
        on_progress: ProgressCallback,
    ) -> AppResult<PolishReply> {
        let params = params([
            ("model", text(request.model)),
            ("system", text(request.system)),
            ("shots", Some(shot_values(request.shots))),
            ("user", text(request.user)),
            ("maxTokens", request.max_tokens.map(Value::from)),
            (
                "temperature",
                request
                    .temperature
                    .map(|value| Value::from(f64::from(value))),
            ),
        ]);
        self.request_cancellable("generate", params, Some(on_progress), GENERATE)
            .await
    }

    /// Tells the overlay pill what to show and how long a finished dictation stays before the
    /// pill settles back to idle; see [`EngineClient::notify`].
    pub fn overlay_state(&self, state: &DictationState, done_hold_ms: u64) -> AppResult<()> {
        let state = serde_json::to_value(state).map_err(AppError::engine)?;
        self.notify(
            "overlay_state",
            params([
                ("state", Some(state)),
                ("doneHoldMs", Some(Value::from(done_hold_ms))),
            ]),
        )
    }

    /// Shows the pill under the cursor, or hides it.
    pub fn overlay_visible(&self, visible: bool) -> AppResult<()> {
        self.notify(
            "overlay_visible",
            params([("visible", Some(Value::from(visible)))]),
        )
    }

    /// Feeds the pill's level meter (RMS, 0..=1).
    pub fn overlay_level(&self, level: f32) -> AppResult<()> {
        self.notify(
            "overlay_level",
            params([("level", Some(Value::from(f64::from(level))))]),
        )
    }

    /// A command whose result carries nothing (`{}`).
    pub(crate) async fn command(
        &self,
        cmd: &str,
        params: Value,
        timeout: Duration,
    ) -> AppResult<()> {
        self.request::<IgnoredAny>(cmd, params, timeout).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;
    use sv_domain::PermissionStatus;

    use super::*;
    use crate::client::Transport;

    /// Answers every request with a canned reply and records the request.
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
    async fn transcribe_sends_camel_case_params_and_parses_reply() {
        let (client, transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"text":"hello","language":"en"}}"#);
        let reply = client
            .transcribe(
                "parakeet-tdt-v3",
                Path::new("/tmp/a.wav"),
                TranscribeOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(
            reply,
            EngineTranscript {
                text: "hello".to_owned(),
                language: Some("en".to_owned()),
                quality: TranscriptQuality::default(),
                segments: Vec::new(),
                dictionary_fixes: 0,
            }
        );
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["cmd"], "transcribe");
        assert_eq!(request["wavPath"], "/tmp/a.wav");
        for omitted in [
            "language",
            "languages",
            "prompt",
            "vocabulary",
            "timestamps",
        ] {
            assert!(request.get(omitted).is_none(), "{omitted}");
        }
    }

    #[tokio::test]
    async fn timed_transcription_asks_for_and_reads_segments() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"text":"Hi there. Bye.","language":"en",
                "segments":[{"startMs":0,"endMs":900,"text":"Hi there."},
                            {"startMs":2400,"endMs":2800,"text":"Bye."}]}}"#,
        );
        let options = TranscribeOptions {
            timestamps: true,
            ..TranscribeOptions::default()
        };
        let reply = client
            .transcribe("parakeet-tdt-v3", Path::new("/tmp/m.wav"), options)
            .await
            .unwrap();
        assert_eq!(
            reply.segments,
            vec![
                TimedSegment {
                    start_ms: 0,
                    end_ms: 900,
                    text: "Hi there.".to_owned()
                },
                TimedSegment {
                    start_ms: 2400,
                    end_ms: 2800,
                    text: "Bye.".to_owned()
                },
            ]
        );
        assert_eq!(transport.seen.lock()[0]["timestamps"], true);
    }

    #[tokio::test]
    async fn transcribe_sends_whisper_options_and_reads_quality() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"text":"kal milte hain","language":"hi",
                "quality":{"avgLogprob":-0.42,"compressionRatio":1.3,"noSpeechProb":0.05}}}"#,
        );
        let languages = vec!["en".to_owned(), "hi".to_owned()];
        let options = TranscribeOptions {
            language: None,
            languages: &languages,
            prompt: "Kubernetes, Anjali",
            vocabulary: &[],
            timestamps: false,
        };
        let reply = client
            .transcribe("whisper-hinglish", Path::new("/tmp/a.wav"), options)
            .await
            .unwrap();
        assert_eq!(
            reply.quality,
            TranscriptQuality {
                avg_logprob: Some(-0.42),
                compression_ratio: Some(1.3),
                no_speech_prob: Some(0.05),
                confidence: None,
            }
        );
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["languages"], serde_json::json!(["en", "hi"]));
        assert_eq!(request["prompt"], "Kubernetes, Anjali");
        assert!(request.get("language").is_none());
    }

    #[tokio::test]
    async fn transcribe_sends_the_dictionary_to_parakeet_and_reads_its_fixes() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"text":"Ask Ujjwal","language":"en",
                "dictionaryFixes":1}}"#,
        );
        let vocabulary = vec!["Ujjwal".to_owned(), "Himanshu".to_owned()];
        let options = TranscribeOptions {
            vocabulary: &vocabulary,
            ..TranscribeOptions::default()
        };
        let reply = client
            .transcribe("parakeet-tdt-v3", Path::new("/tmp/a.wav"), options)
            .await
            .unwrap();
        assert_eq!(reply.dictionary_fixes, 1);
        let request = transport.seen.lock()[0].clone();
        assert_eq!(
            request["vocabulary"],
            serde_json::json!(["Ujjwal", "Himanshu"])
        );
    }

    #[tokio::test]
    async fn permissions_map_to_domain_statuses() {
        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"microphone":"granted",
                "accessibility":"denied","speech":"not_determined","systemAudio":"granted"}}"#,
        );
        let permissions = client.permissions().await.unwrap();
        assert_eq!(permissions.microphone, PermissionStatus::Granted);
        assert_eq!(permissions.accessibility, PermissionStatus::Denied);
        assert_eq!(permissions.speech, PermissionStatus::NotDetermined);
        assert_eq!(permissions.system_audio, PermissionStatus::Granted);
    }

    #[tokio::test]
    async fn model_status_reads_optional_fields() {
        let (client, transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"status":"unsupported","reason":"needs macOS 26"}}"#,
        );
        let status = client
            .model_status("apple-speech", Some("hi"))
            .await
            .unwrap();
        assert_eq!(status.status, ModelStatus::Unsupported);
        assert_eq!(status.size_bytes, None);
        assert_eq!(status.reason.as_deref(), Some("needs macOS 26"));
        assert_eq!(transport.seen.lock()[0]["language"], "hi");
    }

    #[tokio::test]
    async fn paste_without_accessibility_is_a_permission_error() {
        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":false,"error":"accessibility permission missing"}"#);
        assert!(matches!(client.paste().await, Err(AppError::Permission(_))));
    }

    #[tokio::test]
    async fn selected_text_reads_a_selection_or_none() {
        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"text":"hi there"}}"#);
        assert_eq!(
            client.selected_text().await.unwrap().as_deref(),
            Some("hi there")
        );
        assert_eq!(transport.seen.lock()[0]["cmd"], "selected_text");

        let (client, _transport) = scripted(r#"{"id":$ID,"ok":true,"result":{}}"#);
        assert_eq!(client.selected_text().await.unwrap(), None);

        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":false,"error":"accessibility permission missing"}"#);
        assert!(matches!(
            client.selected_text().await,
            Err(AppError::Permission(_))
        ));
    }

    #[tokio::test]
    async fn empty_results_and_muting() {
        let (client, _transport) = scripted(r#"{"id":$ID,"ok":true,"result":{}}"#);
        client.delete_model("parakeet-tdt-v2").await.unwrap();
        client.open_settings(PermissionKind::Speech).await.unwrap();
        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{"previous":true}}"#);
        assert!(client.set_output_muted(true).await.unwrap());
        assert_eq!(transport.seen.lock()[0]["muted"], true);
    }

    #[tokio::test]
    async fn polish_sends_shots_as_objects() {
        let (client, transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"text":"Hi.","finished":true}}"#);
        let shots = vec![("hey".to_owned(), "Hey.".to_owned())];
        let reply = client
            .polish(None, "be formal", &shots, "hi")
            .await
            .unwrap();
        assert_eq!(
            reply,
            PolishReply {
                text: "Hi.".to_owned(),
                finished: true
            }
        );
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["shots"][0]["assistant"], "Hey.");
        assert_eq!(request["system"], "be formal");
        assert!(request.get("model").is_none());
    }

    #[tokio::test]
    async fn polish_and_generate_name_the_local_model() {
        let (client, transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"text":"Résumé","finished":false}}"#);
        client
            .polish(Some("qwen3-4b-instruct"), "be formal", &[], "hi")
            .await
            .unwrap();
        let request = GenerateRequest {
            model: "qwen3-4b-instruct",
            system: "Summarize the meeting.",
            shots: &[],
            user: "[00:01] S1: hello",
            max_tokens: Some(400),
            temperature: Some(0.0),
        };
        let reply = client
            .generate(&request, Box::new(|_, _| {}))
            .await
            .unwrap();
        assert!(!reply.finished);
        let seen = transport.seen.lock().clone();
        assert_eq!(seen[0]["model"], "qwen3-4b-instruct");
        assert_eq!(seen[1]["cmd"], "generate");
        assert_eq!(seen[1]["maxTokens"], 400);
        assert_eq!(seen[1]["temperature"], 0.0);
        assert_eq!(seen[1]["shots"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn model_status_reads_catalog_fields() {
        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"status":"not_downloaded","sizeBytes":2497281120,
                "streaming":false,"contextTokens":8192,"license":"Apache-2.0",
                "source":"https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF",
                "revision":"a06e946bb6b655725eafa393f4a9745d460374c9"}}"#,
        );
        let status = client
            .model_status("qwen3-4b-instruct", None)
            .await
            .unwrap();
        assert_eq!(status.size_bytes, Some(2_497_281_120));
        assert_eq!(status.context_tokens, Some(8192));
        assert_eq!(status.license.as_deref(), Some("Apache-2.0"));
        assert!(!status.streaming);
    }

    #[tokio::test]
    async fn download_requires_ready_status() {
        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"status":"not_downloaded"}}"#);
        let result = client
            .download_model("parakeet-flash", None, Box::new(|_, _| {}))
            .await;
        assert!(matches!(result, Err(AppError::Engine(_))));
    }

    #[tokio::test]
    async fn prepare_edit_watch_sends_the_command_and_maps_missing_trust() {
        let (client, transport) = scripted(r#"{"id":$ID,"ok":true,"result":{}}"#);
        client.prepare_edit_watch().await.unwrap();
        assert_eq!(transport.seen.lock()[0]["cmd"], "prepare_edit_watch");

        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":false,"error":"accessibility permission missing"}"#);
        assert!(matches!(
            client.prepare_edit_watch().await,
            Err(AppError::Permission(_))
        ));
    }

    #[tokio::test]
    async fn watch_edits_sends_text_and_window_and_reads_the_snapshot() {
        let (client, transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"text":"Wispr Flow is great"}}"#);
        let watch = client
            .watch_edits("Whisper Flow is great", Duration::from_millis(1500))
            .await
            .unwrap();
        assert_eq!(
            watch,
            EditWatch {
                text: Some("Wispr Flow is great".to_owned()),
                reason: None,
            }
        );
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["cmd"], "watch_edits");
        assert_eq!(request["text"], "Whisper Flow is great");
        assert_eq!(request["timeoutMs"], 1500);
    }

    #[tokio::test]
    async fn watch_edits_reports_why_nothing_was_read() {
        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":true,"result":{"reason":"secure field"}}"#);
        let watch = client
            .watch_edits("hi", Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(watch.text, None);
        assert_eq!(watch.reason.as_deref(), Some("secure field"));

        let (client, _transport) =
            scripted(r#"{"id":$ID,"ok":false,"error":"accessibility permission missing"}"#);
        assert!(matches!(
            client.watch_edits("hi", Duration::from_secs(60)).await,
            Err(AppError::Permission(_))
        ));
    }
}
