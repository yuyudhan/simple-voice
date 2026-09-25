// FilePath: src-tauri/src/features/models/catalog.rs
//! The fixed list of models Settings → Models offers. Live status comes from the engine helper.

use sv_domain::models::{
    APPLE_INTELLIGENCE, APPLE_SPEECH, GROQ_WHISPER, PARAKEET_FLASH, PARAKEET_TDT_V2,
    PARAKEET_TDT_V3,
};
use sv_domain::{ModelKind, ModelProvider};

/// Id of the Groq post-processing entry (the transcription model is `groq-whisper`).
pub(crate) const GROQ_LLM: &str = "groq";
/// Id of the custom OpenAI-compatible post-processing entry.
pub(crate) const CUSTOM_LLM: &str = "custom";

#[derive(Debug, Clone, Copy)]
pub(crate) struct CatalogEntry {
    pub(crate) id: &'static str,
    pub(crate) kind: ModelKind,
    pub(crate) provider: ModelProvider,
    pub(crate) name: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) speed: u8,
    pub(crate) accuracy: u8,
    pub(crate) languages: &'static str,
    pub(crate) size_mb: Option<u32>,
    /// Remote: nothing to download, always available.
    pub(crate) cloud: bool,
}

pub(crate) const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: GROQ_WHISPER,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Groq,
        name: "Groq Whisper",
        subtitle: "Whisper Large v3 Turbo on Groq — needs an API key",
        speed: 95,
        accuracy: 90,
        languages: "100+ languages",
        size_mb: None,
        cloud: true,
    },
    CatalogEntry {
        id: PARAKEET_TDT_V3,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        name: "Parakeet TDT v3",
        subtitle: "Blazing Fast — Multilingual",
        speed: 100,
        accuracy: 92,
        languages: "25 European languages",
        size_mb: Some(480),
        cloud: false,
    },
    CatalogEntry {
        id: PARAKEET_TDT_V2,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        name: "Parakeet TDT v2",
        subtitle: "Blazing Fast — English",
        speed: 100,
        accuracy: 96,
        languages: "English",
        size_mb: Some(480),
        cloud: false,
    },
    CatalogEntry {
        id: PARAKEET_FLASH,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        name: "Flash Dictation (Beta)",
        subtitle: "Streaming Parakeet for the lowest latency",
        speed: 100,
        accuracy: 75,
        languages: "English",
        size_mb: Some(250),
        cloud: false,
    },
    CatalogEntry {
        id: APPLE_SPEECH,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Apple,
        name: "Apple Speech",
        subtitle: "On-device, built into macOS 26+",
        speed: 85,
        accuracy: 80,
        languages: "On-device locales",
        size_mb: None,
        cloud: false,
    },
    CatalogEntry {
        id: GROQ_LLM,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Groq,
        name: "Groq",
        subtitle: "Fast cloud formatting with the Groq API key",
        speed: 95,
        accuracy: 92,
        languages: "Multilingual",
        size_mb: None,
        cloud: true,
    },
    CatalogEntry {
        id: APPLE_INTELLIGENCE,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Apple,
        name: "Apple Intelligence",
        subtitle: "On-device formatting, macOS 26+",
        speed: 80,
        accuracy: 80,
        languages: "Apple Intelligence languages",
        size_mb: None,
        cloud: false,
    },
    CatalogEntry {
        id: CUSTOM_LLM,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Custom,
        name: "Custom",
        subtitle: "Any OpenAI-compatible endpoint (Ollama, LM Studio, …)",
        speed: 70,
        accuracy: 85,
        languages: "Depends on the model",
        size_mb: None,
        cloud: true,
    },
];
