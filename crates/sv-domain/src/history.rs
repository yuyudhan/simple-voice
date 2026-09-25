// FilePath: crates/sv-domain/src/history.rs
//! Dictation history rows.

use serde::{Deserialize, Serialize};

use crate::{AppCategory, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStatus {
    /// Formatted by the post-processing provider (or it was off / skipped as too short) and pasted.
    Pasted,
    /// Post-processing failed; the deterministic text was pasted instead.
    Unformatted,
    /// Transcription failed; audio is kept for retry.
    Failed,
    /// A newer dictation was already pasted, so this older result was not pasted.
    Dropped,
}

impl HistoryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pasted => "pasted",
            Self::Unformatted => "unformatted",
            Self::Failed => "failed",
            Self::Dropped => "dropped",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pasted" => Some(Self::Pasted),
            "unformatted" => Some(Self::Unformatted),
            "failed" => Some(Self::Failed),
            "dropped" => Some(Self::Dropped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    /// Unix milliseconds when recording started.
    pub created_at: i64,
    pub status: HistoryStatus,
    pub raw_text: String,
    /// What was pasted (or would have been).
    pub text: String,
    pub error: Option<String>,
    pub model: String,
    pub language: Option<String>,
    pub style: Style,
    pub audio_ms: i64,
    pub latency_ms: i64,
    pub word_count: i64,
    pub dictionary_fixes: i64,
    pub words_corrected: i64,
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    pub app_category: AppCategory,
    /// A failed entry whose audio is still on disk.
    pub can_retry: bool,
}

/// Everything the coordinator knows about a finished dictation. Storage derives `word_count`,
/// `words_corrected` and `app_category` from these fields.
#[derive(Debug, Clone, PartialEq)]
pub struct NewHistory {
    pub created_at: i64,
    pub status: HistoryStatus,
    pub raw_text: String,
    pub final_text: String,
    pub error: Option<String>,
    pub model: String,
    pub language: Option<String>,
    pub style: Style,
    pub audio_ms: i64,
    pub latency_ms: i64,
    pub dictionary_fixes: i64,
    pub app_name: Option<String>,
    pub bundle_id: Option<String>,
    /// Retained WAV for a failed dictation; `None` otherwise.
    pub audio_path: Option<String>,
}
