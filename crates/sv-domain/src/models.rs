// FilePath: crates/sv-domain/src/models.rs
//! Transcription and post-processing models shown in Settings → Models.

use serde::{Deserialize, Serialize};

pub const GROQ_WHISPER: &str = "groq-whisper";
pub const PARAKEET_TDT_V3: &str = "parakeet-tdt-v3";
pub const PARAKEET_TDT_V2: &str = "parakeet-tdt-v2";
pub const PARAKEET_FLASH: &str = "parakeet-flash";
pub const APPLE_SPEECH: &str = "apple-speech";
pub const APPLE_INTELLIGENCE: &str = "apple-intelligence";
/// The Groq post-processing entry (the transcription model is `groq-whisper`).
pub const GROQ_LLM: &str = "groq";
/// The custom OpenAI-compatible post-processing entry.
pub const CUSTOM_LLM: &str = "custom";

/// Transcription model ids the helper handles (everything except Groq).
pub const LOCAL_TRANSCRIPTION_MODELS: &[&str] = &[
    PARAKEET_TDT_V3,
    PARAKEET_TDT_V2,
    PARAKEET_FLASH,
    APPLE_SPEECH,
];

/// Display name of a catalog model id. History records a Groq or custom formatting pass by the
/// provider's own model id (e.g. `qwen/qwen3.8-27b`), which has no entry here.
pub fn model_name(id: &str) -> Option<&'static str> {
    match id {
        GROQ_WHISPER => Some("Groq Whisper"),
        PARAKEET_TDT_V3 => Some("Parakeet TDT v3"),
        PARAKEET_TDT_V2 => Some("Parakeet TDT v2"),
        PARAKEET_FLASH => Some("Flash Dictation (Beta)"),
        APPLE_SPEECH => Some("Apple Speech"),
        APPLE_INTELLIGENCE => Some("Apple Intelligence"),
        GROQ_LLM => Some("Groq"),
        CUSTOM_LLM => Some("Custom"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Transcription,
    PostProcessing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Groq,
    Parakeet,
    Apple,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// Remote; nothing to download.
    Cloud,
    Ready,
    NotDownloaded,
    Downloading,
    /// Not available on this Mac (OS version, Apple Intelligence off, ...); see `reason`.
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub kind: ModelKind,
    pub provider: ModelProvider,
    pub name: String,
    pub subtitle: String,
    /// 0..=100
    pub speed: u8,
    /// 0..=100
    pub accuracy: u8,
    pub languages: String,
    pub size_mb: Option<u32>,
    pub status: ModelStatus,
    pub reason: Option<String>,
    /// 0.0..=1.0 while downloading.
    pub progress: Option<f32>,
}
