// FilePath: src-tauri/src/features/models/catalog.rs
//! The fixed list of models Settings → Transcription (voice models) and Settings → Formatting
//! (post-processing models) offer. Live status comes from the engine helper; display names come
//! from `sv_domain::models::model_name`, which history shares.

use sv_domain::languages::Language;
use sv_domain::models::{
    APPLE_INTELLIGENCE, APPLE_SPEECH, CUSTOM_LLM, GROQ_LLM, GROQ_WHISPER, PARAKEET_FLASH,
    PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH, WHISPER_TURBO,
};
use sv_domain::{ModelKind, ModelProvider};

/// The dictation languages a local model writes the way the user chose them (language and
/// script); what Smart Select routes by.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Coverage {
    /// Exactly these registry tags.
    Tags(&'static [&'static str]),
    /// Every language in its own script, none of the romanised variants.
    NativeScripts,
}

impl Coverage {
    pub(crate) fn covers(self, language: &Language) -> bool {
        match self {
            Self::Tags(tags) => tags.contains(&language.tag),
            Self::NativeScripts => !language.is_variant(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CatalogEntry {
    pub(crate) id: &'static str,
    pub(crate) kind: ModelKind,
    pub(crate) provider: ModelProvider,
    pub(crate) subtitle: &'static str,
    pub(crate) speed: u8,
    pub(crate) accuracy: u8,
    pub(crate) languages: &'static str,
    pub(crate) size_mb: Option<u32>,
    /// Remote: nothing to download, always available.
    pub(crate) cloud: bool,
    /// `None`: never a Smart Select candidate (cloud, streaming, locale-bound or LLM models).
    pub(crate) covers: Option<Coverage>,
}

/// Languages Parakeet TDT v3 transcribes.
const PARAKEET_V3_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

pub(crate) const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: GROQ_WHISPER,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Groq,
        subtitle: "Whisper Large v3 Turbo on Groq — needs an API key",
        speed: 95,
        accuracy: 90,
        languages: "100+ languages",
        size_mb: None,
        cloud: true,
        covers: None,
    },
    CatalogEntry {
        id: PARAKEET_TDT_V3,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        subtitle: "Blazing Fast — Multilingual",
        speed: 100,
        accuracy: 92,
        languages: "25 European languages",
        size_mb: Some(480),
        cloud: false,
        covers: Some(Coverage::Tags(PARAKEET_V3_LANGUAGES)),
    },
    CatalogEntry {
        id: PARAKEET_TDT_V2,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        subtitle: "Blazing Fast — English",
        speed: 100,
        accuracy: 96,
        languages: "English",
        size_mb: Some(480),
        cloud: false,
        covers: Some(Coverage::Tags(&["en"])),
    },
    CatalogEntry {
        id: PARAKEET_FLASH,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        subtitle: "Streaming Parakeet for the lowest latency",
        speed: 100,
        accuracy: 75,
        languages: "English",
        size_mb: Some(250),
        cloud: false,
        covers: None,
    },
    CatalogEntry {
        id: WHISPER_TURBO,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Whisper,
        subtitle: "Whisper Large v3 Turbo on this Mac — Multilingual",
        speed: 70,
        accuracy: 90,
        languages: "99 languages incl. Hindi",
        size_mb: Some(1640),
        cloud: false,
        covers: Some(Coverage::NativeScripts),
    },
    CatalogEntry {
        id: WHISPER_HINGLISH,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Whisper,
        subtitle: "Whisper tuned for Hinglish — writes Hindi in Latin script",
        speed: 55,
        accuracy: 92,
        languages: "Hindi, Hinglish (romanised) and English",
        size_mb: Some(3100),
        cloud: false,
        covers: Some(Coverage::Tags(&["en", "hi-Latn"])),
    },
    CatalogEntry {
        id: APPLE_SPEECH,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Apple,
        subtitle: "On-device, built into macOS 26+",
        speed: 85,
        accuracy: 80,
        languages: "On-device locales",
        size_mb: None,
        cloud: false,
        covers: None,
    },
    CatalogEntry {
        id: GROQ_LLM,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Groq,
        subtitle: "Fast cloud formatting with the Groq API key",
        speed: 95,
        accuracy: 92,
        languages: "Multilingual",
        size_mb: None,
        cloud: true,
        covers: None,
    },
    CatalogEntry {
        id: APPLE_INTELLIGENCE,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Apple,
        subtitle: "On-device formatting, macOS 26+",
        speed: 80,
        accuracy: 80,
        languages: "Apple Intelligence languages",
        size_mb: None,
        cloud: false,
        covers: None,
    },
    CatalogEntry {
        id: CUSTOM_LLM,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Custom,
        subtitle: "Any OpenAI-compatible endpoint (Ollama, LM Studio, …)",
        speed: 70,
        accuracy: 85,
        languages: "Depends on the model",
        size_mb: None,
        cloud: true,
        covers: None,
    },
];
