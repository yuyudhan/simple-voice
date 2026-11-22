// FilePath: crates/sv-text/src/speculation/mod.rs
//! Formatting while the user still speaks. Every few seconds the recording so far is
//! transcribed again with the whole pass's model and options (a "draft"); words two drafts agree
//! on have settled, and closed sentence runs of them ("segments") go through the LLM formatting
//! pass while the recording continues, with the segments before it as read-only context. At stop
//! the pasted words still come from transcribing the whole recording and the deterministic
//! formatter still runs on the whole text: a formatted segment is reused only where its input is
//! the whole text's next stretch, token for token, so a draft that heard or punctuated something
//! differently costs speed, never accuracy. Only the rest (the "tail") is formatted after the stop.
//!
//! Everything here is pure: the app's worker (`src-tauri/.../dictation/speculative.rs`) and the
//! latency bench (`examples/latency/`) drive the same [`Speculator`], [`plan`] and guards.

mod plan;
mod prompt;
mod speculator;

pub use plan::{join, plan, Plan};
pub use prompt::{accept_segment, polish_segment_prompt};
pub use speculator::{draft_due, Formatted, Request, Speculator, MIN_SEGMENT_WORDS};

/// The word as matched across texts: letters and digits only, lowercased, so punctuation and
/// capitals the formatting pass decides anyway never break a match. A token without any (a
/// lone dash) has no word.
fn match_word(token: &str) -> String {
    token
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The match words of `text`, in order, without the tokens that have none.
fn match_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(match_word)
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_words_ignore_case_and_punctuation() {
        assert_eq!(
            match_words("Send it to ankit@sienzo.com, - then v1.2!"),
            ["send", "it", "to", "ankitsienzocom", "then", "v12"]
        );
    }
}
