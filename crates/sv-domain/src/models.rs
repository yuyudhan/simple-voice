// FilePath: crates/sv-domain/src/models.rs
//! On-device models: transcription models shown in Settings → Transcription, language models
//! shown in Settings → Formatting (and used by meetings), and the speaker diarization model
//! meetings use. Every model runs on the Mac; downloads carry no user content.

use serde::{Deserialize, Serialize};

pub const PARAKEET_TDT_V3: &str = "parakeet-tdt-v3";
pub const PARAKEET_TDT_V2: &str = "parakeet-tdt-v2";
pub const PARAKEET_FLASH: &str = "parakeet-flash";
pub const APPLE_SPEECH: &str = "apple-speech";
/// Picks the voice model per dictation from the languages and what is downloaded.
pub const SMART_SELECT: &str = "smart-select";
pub const WHISPER_TURBO: &str = "whisper-large-v3-turbo";
/// Whisper fine-tuned to write Hindi and Hinglish speech in romanised Hinglish.
pub const WHISPER_HINGLISH: &str = "whisper-hinglish";
pub const APPLE_INTELLIGENCE: &str = "apple-intelligence";
/// The downloadable local language model (formatting, edit mode, learning, meetings).
pub const LOCAL_LLM: &str = "qwen3-4b-instruct";
/// The speaker diarization model meetings use.
pub const DIARIZER: &str = "speaker-diarization";
/// The dedicated translation model meeting translations use (MADLAD-400 3B MT).
pub const TRANSLATOR: &str = "madlad400-3b-mt";
/// The removed cloud transcription model. Only history rows recorded before the app became
/// local-only carry it; it is never offered or run.
pub const LEGACY_GROQ_WHISPER: &str = "groq-whisper";

/// Transcription model ids the helper handles.
pub const LOCAL_TRANSCRIPTION_MODELS: &[&str] = &[
    PARAKEET_TDT_V3,
    PARAKEET_TDT_V2,
    PARAKEET_FLASH,
    APPLE_SPEECH,
    WHISPER_TURBO,
    WHISPER_HINGLISH,
];

/// Model ids with files the helper downloads and deletes on request.
pub const DOWNLOADABLE_MODELS: &[&str] = &[
    PARAKEET_TDT_V3,
    PARAKEET_TDT_V2,
    PARAKEET_FLASH,
    APPLE_SPEECH,
    WHISPER_TURBO,
    WHISPER_HINGLISH,
    LOCAL_LLM,
    DIARIZER,
    TRANSLATOR,
];

/// Display name of a model id, including ids only old history rows carry. A formatting pass
/// recorded by a removed provider's own model id (e.g. `qwen/qwen3.8-27b`) has no entry here.
pub fn model_name(id: &str) -> Option<&'static str> {
    match id {
        SMART_SELECT => Some("Smart Select"),
        WHISPER_TURBO => Some("Whisper Turbo"),
        WHISPER_HINGLISH => Some("Hinglish Whisper"),
        PARAKEET_TDT_V3 => Some("Parakeet TDT v3"),
        PARAKEET_TDT_V2 => Some("Parakeet TDT v2"),
        PARAKEET_FLASH => Some("Flash Dictation (Beta)"),
        APPLE_SPEECH => Some("Apple Speech"),
        APPLE_INTELLIGENCE => Some("Apple Intelligence"),
        LOCAL_LLM => Some("Qwen3 4B Instruct"),
        DIARIZER => Some("Speaker Diarization"),
        TRANSLATOR => Some("MADLAD-400 Translation"),
        LEGACY_GROQ_WHISPER => Some("Groq Whisper (removed)"),
        "groq" => Some("Groq (removed)"),
        "custom" => Some("Custom endpoint (removed)"),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    Transcription,
    /// A language model: formatting, edit mode, learning and meetings.
    PostProcessing,
    /// Tells speakers apart in meeting audio.
    Diarization,
    /// Translates meeting transcripts.
    Translation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Parakeet,
    Apple,
    Whisper,
    /// The downloadable local language model.
    Llm,
    /// The speaker diarization model.
    Diarizer,
    /// The dedicated translation model.
    Translator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
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
    /// License of the model weights; `None` for models built into macOS.
    pub license: Option<String>,
    /// Where the download comes from; `None` for models built into macOS.
    pub source: Option<String>,
    /// Produces a live transcript while recording (incremental recognition).
    pub streaming: bool,
}

/// What Smart Select would run for the current languages, shown under its settings card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartSelectPlan {
    /// Model id of the model a dictation runs now: the best downloaded one for the languages,
    /// else (nothing ready) `recommended`.
    pub local: String,
    /// Model id of the best model for the languages, whether downloaded or not.
    pub recommended: String,
    /// `local` is downloaded and can run.
    pub ready: bool,
    /// Set only when the local model can't write a selected language the way it was chosen.
    pub notice: Option<String>,
}
