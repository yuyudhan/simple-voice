// FilePath: crates/sv-text/src/lib.rs
//! Text processing between the speech model and the paste: the recognition prompt built from
//! the personal dictionary, the deterministic formatter that always runs, and the prompt and
//! guards for the optional LLM formatting pass. Everything here is pure and fast, because it
//! sits on the latency path of every dictation.
#![forbid(unsafe_code)]

pub mod formatting;
pub mod polish;
pub mod vocabulary;

pub use formatting::{format, Formatted};
pub use polish::{
    accept_polish, polish_prompt, polish_timeout, should_skip_polish, PolishOutcome, PolishPrompt,
};
pub use vocabulary::{Rule, Vocabulary, PROMPT_MAX_CHARS};
