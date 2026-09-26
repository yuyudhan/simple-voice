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
use sv_domain::{AppError, AppResult, DictationState, ModelStatus, PermissionKind, Permissions};

use crate::client::{EngineClient, ProgressCallback};

const QUICK: Duration = Duration::from_secs(10);
const MODEL_STATUS: Duration = Duration::from_secs(30);
const DOWNLOAD: Duration = Duration::from_secs(2 * 60 * 60);
const PRELOAD: Duration = Duration::from_secs(5 * 60);
const TRANSCRIBE: Duration = Duration::from_secs(120);
/// The user may leave the system prompt open for a while before answering it.
const REQUEST_PERMISSION: Duration = Duration::from_secs(10 * 60);
const POLISH: Duration = Duration::from_secs(30);
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
    pub size_bytes: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineTranscript {
    pub text: String,
    pub language: Option<String>,
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

/// Builds a flat parameter object, leaving out absent optional values.
fn params<const N: usize>(pairs: [(&str, Option<Value>); N]) -> Value {
    let map: Map<String, Value> = pairs
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key.to_owned(), value)))
        .collect();
    Value::Object(map)
}

fn text(value: &str) -> Option<Value> {
    Some(Value::from(value))
}

fn optional_text(value: Option<&str>) -> Option<Value> {
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
            .request_with_progress("download_model", params, on_progress, DOWNLOAD)
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

    /// `wav_path` must be 16 kHz mono PCM16; `language` pins recognition when set.
    pub async fn transcribe(
        &self,
        model: &str,
        wav_path: &Path,
        language: Option<&str>,
    ) -> AppResult<EngineTranscript> {
        let path = wav_path.to_str().ok_or_else(|| {
            AppError::invalid(format!(
                "The recording path is not valid UTF-8: {}",
                wav_path.display()
            ))
        })?;
        let params = params([
            ("model", text(model)),
            ("wavPath", text(path)),
            ("language", optional_text(language)),
        ]);
        self.request("transcribe", params, TRANSCRIBE).await
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

    /// Runs the post-processing prompt on Apple Intelligence. `shots` are `(user, assistant)`
    /// example pairs.
    pub async fn polish(
        &self,
        system: &str,
        shots: &[(String, String)],
        user: &str,
    ) -> AppResult<PolishReply> {
        let shots: Vec<Value> = shots
            .iter()
            .map(|(example_user, example_assistant)| {
                params([
                    ("user", text(example_user)),
                    ("assistant", text(example_assistant)),
                ])
            })
            .collect();
        let params = params([
            ("system", text(system)),
            ("shots", Some(Value::Array(shots))),
            ("user", text(user)),
        ]);
        self.request("polish", params, POLISH).await
    }

    /// Tells the overlay pill what to show; see [`EngineClient::notify`].
    pub fn overlay_state(&self, state: &DictationState) -> AppResult<()> {
        let state = serde_json::to_value(state).map_err(AppError::engine)?;
        self.notify("overlay_state", params([("state", Some(state))]))
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
    async fn command(&self, cmd: &str, params: Value, timeout: Duration) -> AppResult<()> {
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
            let id = request["id"].as_u64().unwrap();
            self.seen.lock().push(request);
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
            .transcribe("parakeet-tdt-v3", Path::new("/tmp/a.wav"), None)
            .await
            .unwrap();
        assert_eq!(
            reply,
            EngineTranscript {
                text: "hello".to_owned(),
                language: Some("en".to_owned())
            }
        );
        let request = transport.seen.lock()[0].clone();
        assert_eq!(request["cmd"], "transcribe");
        assert_eq!(request["wavPath"], "/tmp/a.wav");
        assert!(request.get("language").is_none());
    }

    #[tokio::test]
    async fn permissions_map_to_domain_statuses() {
        let (client, _transport) = scripted(
            r#"{"id":$ID,"ok":true,"result":{"microphone":"granted",
                "accessibility":"denied","speech":"not_determined"}}"#,
        );
        let permissions = client.permissions().await.unwrap();
        assert_eq!(permissions.microphone, PermissionStatus::Granted);
        assert_eq!(permissions.accessibility, PermissionStatus::Denied);
        assert_eq!(permissions.speech, PermissionStatus::NotDetermined);
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
        let reply = client.polish("be formal", &shots, "hi").await.unwrap();
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
