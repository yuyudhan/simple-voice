// FilePath: crates/sv-domain/src/dictionary.rs
//! Personal dictionary rows. A row without a replacement is a word to recognise and spell
//! exactly; a row with one is a rule: `phrase` (as heard) is written as `replacement`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: i64,
    pub phrase: String,
    pub replacement: Option<String>,
    /// Unix milliseconds.
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub added: u32,
    /// Duplicates, blank lines and comments are not counted; only lines that parsed but were
    /// already present.
    pub skipped: u32,
}
