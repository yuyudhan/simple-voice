// FilePath: engine-linux/src/linux/transcript.rs
//! The `transcribe` request and reply both runtimes share (docs/internal/engine.md), so the
//! router sends one request shape to Parakeet or Whisper by model id.

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranscribeParams {
    pub(crate) model: String,
    pub(crate) wav_path: String,
    pub(crate) language: Option<String>,
    #[serde(default)]
    pub(crate) languages: Vec<String>,
    #[serde(default)]
    pub(crate) prompt: String,
    #[serde(default)]
    pub(crate) timestamps: bool,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Transcript {
    pub(crate) text: String,
    pub(crate) language: Option<String>,
    pub(crate) quality: Quality,
    pub(crate) segments: Vec<Segment>,
    pub(crate) dictionary_fixes: u32,
}

/// Smart Select's quality numbers; `None` where a runtime has no such measure.
#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Quality {
    pub(crate) avg_logprob: Option<f32>,
    pub(crate) compression_ratio: Option<f32>,
    pub(crate) no_speech_prob: Option<f32>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Segment {
    pub(crate) start_ms: i64,
    pub(crate) end_ms: i64,
    pub(crate) text: String,
}

/// Reads the request's 16 kHz mono WAV as samples.
pub(crate) async fn read_samples(params: &TranscribeParams) -> Result<Vec<i16>, String> {
    let bytes = tokio::fs::read(&params.wav_path)
        .await
        .map_err(|error| format!("cannot read {}: {error}", params.wav_path))?;
    sv_audio::decode_wav(&bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_request_shape_the_app_sends() {
        let params: TranscribeParams = serde_json::from_value(serde_json::json!({
            "id": 3, "cmd": "transcribe", "model": "whisper-large-v3-turbo",
            "wavPath": "/tmp/a.wav", "languages": ["en"], "prompt": "Priya, Arjun",
            "vocabulary": ["Priya"]
        }))
        .unwrap();
        assert_eq!(params.wav_path, "/tmp/a.wav");
        assert_eq!(params.prompt, "Priya, Arjun");
        assert!(!params.timestamps);
    }
}
