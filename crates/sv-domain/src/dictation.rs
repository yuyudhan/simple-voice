// FilePath: crates/sv-domain/src/dictation.rs
//! Event payloads the core emits to the UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationPhase {
    Idle,
    Recording,
    Transcribing,
    Formatting,
    Done,
    Error,
    Cancelled,
}

/// Payload of the `dictation-state` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationState {
    pub phase: DictationPhase,
    pub session_id: u64,
    /// Unix ms the recording started (recording phase only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    /// Error text or short status line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Start of the pasted text, one line (done phase only; `sv_text::preview`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// e.g. "unformatted" when post-processing fell back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl DictationState {
    pub fn new(phase: DictationPhase, session_id: u64) -> Self {
        Self {
            phase,
            session_id,
            started_at: None,
            message: None,
            text: None,
            note: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProgressStatus {
    Downloading,
    Ready,
    Failed,
}

/// Payload of the `model-progress` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgress {
    pub id: String,
    /// 0.0..=1.0
    pub fraction: f32,
    pub status: ModelProgressStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
