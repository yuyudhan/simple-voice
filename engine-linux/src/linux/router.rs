// FilePath: engine-linux/src/linux/router.rs
//! Routes one request to the feature that implements it. Commands for features that exist only
//! on macOS (Apple models, local language models, live recognition, meetings' diarization and
//! system audio, the correction watch) fail with a reason the app shows, never with a fake
//! result.

use std::path::PathBuf;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use sv_domain::PermissionKind;

use super::output::{Output, Progress};
use super::whisper::{Resident, TranscribeParams};
use super::{autostart, desktop, media, models, VERSION};

const NO_LANGUAGE_MODELS: &str = "On-device language models run on macOS only; on Linux choose \
    Groq for formatting and edit mode";
const NO_LIVE: &str = "Live transcription is not available on Linux";
const NO_MEETING_AUDIO: &str = "Speaker diarization and system audio are not available on Linux";
const NO_EDIT_WATCH: &str = "Learning from corrections needs macOS Accessibility; it is not \
    available on Linux";

#[derive(Debug)]
pub(crate) struct Engine {
    pub(crate) output: Arc<Output>,
    store: models::Store,
    whisper: Resident,
}

impl Engine {
    pub(crate) fn new(models_dir: PathBuf) -> Self {
        Self {
            output: Arc::default(),
            store: models::Store::new(models_dir),
            whisper: Resident::default(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ModelParams {
    model: String,
}

#[derive(Debug, Deserialize)]
struct KindParams {
    kind: PermissionKind,
}

#[derive(Debug, Deserialize)]
struct EnabledParams {
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct MutedParams {
    muted: bool,
}

#[derive(Debug, Deserialize)]
struct PlayersParams {
    #[serde(default)]
    players: Vec<String>,
}

pub(crate) async fn route(
    engine: &Engine,
    cmd: &str,
    params: &Value,
    progress: &Progress,
) -> Result<Value, String> {
    match cmd {
        "ping" => Ok(json!({ "version": VERSION })),
        "model_status" => {
            let ModelParams { model } = parse(params)?;
            to_value(&engine.store.status(&model)?)
        }
        "download_model" => {
            let ModelParams { model } = parse(params)?;
            engine.store.download(&model, progress).await?;
            Ok(json!({ "status": "ready" }))
        }
        "delete_model" => {
            let ModelParams { model } = parse(params)?;
            engine.whisper.unload().await;
            engine.store.delete(&model)?;
            Ok(json!({}))
        }
        "preload" => {
            let ModelParams { model } = parse(params)?;
            engine.whisper.preload(&engine.store, &model).await?;
            Ok(json!({}))
        }
        "transcribe" => {
            let request: TranscribeParams = parse(params)?;
            to_value(&engine.whisper.transcribe(&engine.store, request).await?)
        }
        "permissions" | "request_permission" => to_value(&desktop::permissions()),
        "open_settings" => {
            let KindParams { kind } = parse(params)?;
            desktop::open_settings(kind).await?;
            Ok(json!({}))
        }
        "frontmost_app" => to_value(&desktop::frontmost_app().await),
        "paste" => {
            desktop::paste().await?;
            Ok(json!({}))
        }
        "selected_text" => Ok(desktop::selected_text().await),
        "focus_context" => Ok(desktop::focus_context().await),
        "set_output_muted" => {
            let MutedParams { muted } = parse(params)?;
            media::set_output_muted(muted).await
        }
        "pause_media" => media::pause_media().await,
        "resume_media" => {
            let PlayersParams { players } = parse(params)?;
            media::resume_media(&players).await
        }
        // Linux keyboards report no Fn key to applications.
        "watch_fn_key" => Ok(json!({ "active": false })),
        "login_item" => autostart::status(),
        "set_login_item" => {
            let EnabledParams { enabled } = parse(params)?;
            autostart::set(enabled)
        }
        "prepare_edit_watch" | "watch_edits" => Err(NO_EDIT_WATCH.to_owned()),
        "generate" | "prepare_generate" | "polish" | "translate" => {
            Err(NO_LANGUAGE_MODELS.to_owned())
        }
        "stream_start" | "stream_finish" | "stream_cancel" => Err(NO_LIVE.to_owned()),
        "diarize" | "system_audio_start" | "system_audio_stop" => Err(NO_MEETING_AUDIO.to_owned()),
        other => Err(format!("unknown command `{other}`")),
    }
}

fn parse<T: DeserializeOwned>(params: &Value) -> Result<T, String> {
    T::deserialize(params).map_err(|error| format!("invalid parameters: {error}"))
}

fn to_value(value: &impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn call(cmd: &str, params: Value) -> Result<Value, String> {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::new(dir.path().to_path_buf());
        let progress = Progress::new(1, Arc::clone(&engine.output));
        route(&engine, cmd, &params, &progress).await
    }

    #[tokio::test]
    async fn answers_ping_with_the_version() {
        assert_eq!(
            call("ping", json!({})).await.unwrap(),
            json!({ "version": VERSION })
        );
    }

    #[tokio::test]
    async fn reports_model_status_in_the_helper_shape() {
        let status = call("model_status", json!({ "model": "whisper-large-v3-turbo" }))
            .await
            .unwrap();
        assert_eq!(status["status"], "not_downloaded");
        assert_eq!(status["sizeBytes"], 574_041_195);
        let parakeet = call("model_status", json!({ "model": "parakeet-tdt-v3" }))
            .await
            .unwrap();
        assert_eq!(parakeet["status"], "unsupported");
    }

    #[tokio::test]
    async fn mac_only_features_fail_with_a_reason() {
        for cmd in ["generate", "stream_start", "diarize", "watch_edits"] {
            let error = call(cmd, json!({})).await.unwrap_err();
            assert!(error.contains("Linux"), "{cmd}: {error}");
        }
        assert_eq!(
            call("watch_fn_key", json!({ "enabled": true }))
                .await
                .unwrap(),
            json!({ "active": false })
        );
        assert!(call("frobnicate", json!({}))
            .await
            .unwrap_err()
            .contains("unknown command"));
        assert!(call("model_status", json!({}))
            .await
            .unwrap_err()
            .contains("invalid"));
    }

    #[tokio::test]
    async fn permissions_use_the_app_shape() {
        let permissions = call("permissions", json!({})).await.unwrap();
        assert_eq!(permissions["microphone"], "granted");
        assert_eq!(permissions["speech"], "restricted");
        assert!(permissions.get("systemAudio").is_some());
    }
}
