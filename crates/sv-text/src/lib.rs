// FilePath: crates/sv-text/src/lib.rs
//! Text processing between the speech model and the paste: the recognition prompt built from
//! the personal dictionary, the deterministic formatter that always runs, the prompt and guards
//! for the optional LLM formatting pass, the prompt and checks for edit mode, the one-line
//! preview the pill shows afterwards, and the word diff and prompt that pick which of the user's
//! corrections to learn. Everything here is pure and fast, because it sits on the latency path
//! of every dictation.
#![forbid(unsafe_code)]

pub mod edit;
pub mod formatting;
pub mod learning;
pub mod polish;
pub mod preview;
pub mod vocabulary;

pub use edit::{accept_edit, edit_max_tokens, edit_prompt, edit_timeout, EDIT_MAX_CHARS};
pub use formatting::{format, Formatted};
pub use learning::{
    accept_learning, corrections, learning_prompt, Correction, LEARNING_MAX_TOKENS,
    LEARNING_TIMEOUT, MAX_CORRECTIONS,
};
pub use polish::{
    accept_polish, polish_prompt, polish_timeout, should_skip_polish, PolishOutcome, PolishPrompt,
    PolishTarget,
};
pub use preview::{preview, PREVIEW_MAX_CHARS};
pub use vocabulary::{Rule, Vocabulary, PROMPT_MAX_CHARS};
