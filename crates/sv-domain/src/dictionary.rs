// FilePath: crates/sv-domain/src/dictionary.rs
//! Personal dictionary rows. A row without a replacement is a word to recognise and spell
//! exactly; a row with one is a rule: `phrase` (as heard) is written as `replacement`.

use serde::{Deserialize, Serialize};

/// Who added a dictionary row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictionarySource {
    /// Typed, imported or edited by the user.
    Manual,
    /// Learned from a correction the user made after a paste.
    Learned,
}

impl DictionarySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Learned => "learned",
        }
    }

    /// Unknown values read as `Manual`: a row nobody can attribute is treated as the user's own.
    pub fn parse(value: &str) -> Self {
        match value {
            "learned" => Self::Learned,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: i64,
    pub phrase: String,
    pub replacement: Option<String>,
    /// Unix milliseconds.
    pub created_at: i64,
    pub source: DictionarySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: u32,
    /// Duplicates, blank lines and comments are not counted; only lines that parsed but were
    /// already present.
    pub skipped: u32,
}
