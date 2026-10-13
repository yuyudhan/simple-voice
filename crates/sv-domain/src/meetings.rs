// FilePath: crates/sv-domain/src/meetings.rs
//! Personal meetings: an explicitly started recording of the microphone (and optionally system
//! audio), transcribed with timestamps, split into speakers by a local diarization model, and
//! summarized, questioned and translated by an on-device language model. Everything stays on
//! the Mac.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingStatus {
    Recording,
    /// Transcribing and telling speakers apart.
    Processing,
    Ready,
    Failed,
    Cancelled,
}

impl MeetingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "recording" => Some(Self::Recording),
            "processing" => Some(Self::Processing),
            "ready" => Some(Self::Ready),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    /// Unix ms.
    pub started_at: i64,
    /// Unix ms; `None` while recording.
    pub ended_at: Option<i64>,
    pub duration_ms: i64,
    pub status: MeetingStatus,
    pub microphone: bool,
    pub system_audio: bool,
    /// Language the transcript was detected in, when the model reports one.
    pub language: Option<String>,
    pub error: Option<String>,
    pub segment_count: i64,
    pub speaker_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Speaker {
    pub id: i64,
    /// "Speaker 1", in order of first appearance.
    pub label: String,
    /// The name the user gave; `None` shows `label`.
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub id: i64,
    pub start_ms: i64,
    pub end_ms: i64,
    /// `None` when diarization assigned no speaker to this stretch.
    pub speaker_id: Option<i64>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub text: String,
    pub owner: Option<String>,
    /// Where in the meeting it was agreed, when the model cited a segment.
    pub start_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub text: String,
    pub action_items: Vec<ActionItem>,
    /// Model id that wrote it.
    pub model: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    User,
    Assistant,
}

impl ChatRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "assistant" => Some(Self::Assistant),
            _ => None,
        }
    }
}

/// A transcript segment an answer is grounded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub segment_id: i64,
    pub start_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub id: i64,
    pub role: ChatRole,
    pub text: String,
    pub citations: Vec<Citation>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslatedSegment {
    pub segment_id: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Translation {
    /// Registry tag.
    pub language: String,
    pub model: String,
    pub created_at: i64,
    pub segments: Vec<TranslatedSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetail {
    pub meeting: Meeting,
    pub speakers: Vec<Speaker>,
    pub segments: Vec<Segment>,
    pub summary: Option<MeetingSummary>,
    pub chat: Vec<ChatMessage>,
    pub translations: Vec<Translation>,
}

/// What blocks each meeting stage right now; `None` = ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingReadiness {
    pub transcription: Option<String>,
    pub diarization: Option<String>,
    /// The meeting assistant (summary and chat).
    pub language: Option<String>,
    /// The translation model.
    pub translation: Option<String>,
}

/// Payload of the `meeting-state` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingState {
    pub id: i64,
    pub status: MeetingStatus,
    pub elapsed_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingJob {
    Transcribe,
    Diarize,
    Summarize,
    Chat,
    Translate,
}

/// Payload of the `meeting-progress` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingProgress {
    pub id: i64,
    pub job: MeetingJob,
    /// 0.0..=1.0 when the stage reports it.
    pub fraction: Option<f32>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Markdown,
    Text,
    Json,
}
