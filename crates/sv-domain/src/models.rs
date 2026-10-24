// FilePath: crates/sv-domain/src/models.rs
//! Models: on-device transcription models shown in Settings → Transcription, language models
//! shown in Settings → Formatting (and used by meetings), the speaker diarization model meetings
//! use, and the optional Groq cloud models. Every catalog model runs on the Mac (downloads carry
//! no user content); only the Groq models, which the user opts into, send anything out.

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
/// Groq Whisper (cloud), the id history records for a dictation Groq transcribed. Chosen with
/// `TranscriptionProvider::Groq`, never as `Settings::transcription_model`.
pub const GROQ_WHISPER: &str = "groq-whisper";
/// Groq's Whisper models, best first.
pub const GROQ_WHISPER_MODELS: &[&str] = &["whisper-large-v3-turbo", "whisper-large-v3"];
pub const DEFAULT_GROQ_WHISPER_MODEL: &str = "whisper-large-v3-turbo";
/// The Groq chat model formatting and meetings use until the user names another.
pub const DEFAULT_GROQ_CHAT_MODEL: &str = "qwen/qwen3.8-27b";
/// Language-model ids for Groq chat models are `groq:<Groq model id>`, so history and the
/// language-model layer tell them from on-device ids.
pub const GROQ_LLM_PREFIX: &str = "groq:";

/// Longest Groq chat model id accepted; Groq's own ids are far shorter.
const MAX_GROQ_MODEL_CHARS: usize = 100;

/// A Groq model ID as the Groq console lists them (`qwen/qwen3.8-27b`,
/// `llama-3.3-70b-versatile`): ASCII letters, digits and `-_./:`, at most 100 characters.
pub fn is_groq_model_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_GROQ_MODEL_CHARS
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
}

/// The language-model id of a Groq chat model.
pub fn groq_llm_id(groq_model: &str) -> String {
    format!("{GROQ_LLM_PREFIX}{}", groq_model.trim())
}

/// The Groq chat model a language-model id names; `None` for on-device ids.
pub fn groq_chat_model(id: &str) -> Option<&str> {
    id.strip_prefix(GROQ_LLM_PREFIX)
        .map(str::trim)
        .filter(|model| !model.is_empty())
}

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
/// recorded by a Groq chat model (`groq:<model>`, or a bare provider id such as
/// `qwen/qwen3.8-27b` in older rows) has no entry here.
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
        GROQ_WHISPER => Some("Groq Whisper"),
        "groq" => Some("Groq"),
        "custom" => Some("Custom endpoint (removed)"),
        _ => None,
    }
}

/// The name history and insights show for a model id: its catalog name, `Groq · <model>` for a
/// Groq chat model, else the id itself.
pub fn display_name(id: &str) -> String {
    match (model_name(id), groq_chat_model(id)) {
        (Some(name), _) => name.to_owned(),
        (None, Some(groq)) => format!("Groq · {groq}"),
        (None, None) => id.to_owned(),
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
