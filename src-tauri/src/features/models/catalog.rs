// FilePath: src-tauri/src/features/models/catalog.rs
//! The fixed list of on-device models: voice models (Settings → Transcription), language models
//! (Settings → Formatting; also meetings), the meeting speaker diarization model and Parakeet's
//! dictionary booster (Settings → Transcription). Live status, download size, license and
//! provenance come from the engine helper; display names come from
//! `sv_domain::models::model_name`, which history shares.

use sv_domain::languages::Language;
use sv_domain::models::{
    APPLE_INTELLIGENCE, APPLE_SPEECH, DIARIZER, LOCAL_LLM, PARAKEET_FLASH, PARAKEET_TDT_V2,
    PARAKEET_TDT_V3, PARAKEET_VOCABULARY, TRANSLATOR, WHISPER_HINGLISH, WHISPER_TURBO,
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
    /// Download size shown until the helper reports the real one; `None` for models built
    /// into macOS.
    pub(crate) size_mb: Option<u32>,
    /// `None`: never a Smart Select candidate (streaming-only, locale-bound, LLM and
    /// diarization models).
    pub(crate) covers: Option<Coverage>,
}

/// Languages Parakeet TDT v3 transcribes.
const PARAKEET_V3_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

pub(crate) const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: PARAKEET_TDT_V3,
        kind: ModelKind::Transcription,
        provider: ModelProvider::Parakeet,
        subtitle: "Blazing Fast — Multilingual",
        speed: 100,
        accuracy: 92,
        languages: "25 European languages",
        size_mb: Some(480),
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
        covers: None,
    },
    CatalogEntry {
        id: LOCAL_LLM,
        kind: ModelKind::PostProcessing,
        provider: ModelProvider::Llm,
        subtitle: "Runs on this Mac with llama.cpp — formatting, edits and meetings, offline",
        speed: 60,
        accuracy: 85,
        languages: "Multilingual",
        size_mb: Some(2_497),
        covers: None,
    },
    CatalogEntry {
        id: DIARIZER,
        kind: ModelKind::Diarization,
        provider: ModelProvider::Diarizer,
        subtitle: "Tells meeting speakers apart on this Mac",
        speed: 90,
        accuracy: 80,
        languages: "Any language",
        size_mb: Some(22),
        covers: None,
    },
    CatalogEntry {
        id: TRANSLATOR,
        kind: ModelKind::Translation,
        provider: ModelProvider::Translator,
        subtitle: "Dedicated translation model for meetings, on this Mac",
        speed: 60,
        accuracy: 85,
        languages: "400+ languages",
        size_mb: Some(1_859),
        covers: None,
    },
    CatalogEntry {
        id: PARAKEET_VOCABULARY,
        kind: ModelKind::Vocabulary,
        provider: ModelProvider::Parakeet,
        subtitle: "Spells your Dictionary's names in Parakeet dictations from the audio",
        speed: 90,
        accuracy: 85,
        languages: "English names",
        size_mb: Some(103),
        covers: None,
    },
];
