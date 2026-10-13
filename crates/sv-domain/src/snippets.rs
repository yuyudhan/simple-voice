// FilePath: crates/sv-domain/src/snippets.rs
//! Personal snippets: a short spoken trigger ("my address") that dictation expands into saved
//! text, possibly several lines. Stored locally; never shared.

use serde::{Deserialize, Serialize};

/// Longest trigger, in characters.
pub const SNIPPET_TRIGGER_MAX_CHARS: usize = 60;
/// Most words in a trigger.
pub const SNIPPET_TRIGGER_MAX_WORDS: usize = 6;
/// Longest expansion, in characters.
pub const SNIPPET_TEXT_MAX_CHARS: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub id: i64,
    /// The words that trigger the expansion, matched ignoring case at word boundaries.
    pub trigger: String,
    /// The text pasted in place of the trigger, exactly as saved (whitespace and newlines kept).
    pub text: String,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms.
    pub updated_at: i64,
}
