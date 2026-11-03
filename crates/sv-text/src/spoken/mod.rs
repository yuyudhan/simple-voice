// FilePath: crates/sv-text/src/spoken/mod.rs
//! Spoken forms the speech model writes out word by word, rewritten as they are typed: email
//! addresses ("ankit at the rate sienzo dot com" → `ankit@sienzo.com`), key chords ("control
//! alt delete" → `Ctrl+Alt+Delete`), arithmetic ("two plus two equal to four" → `2 + 2 = 4`),
//! technical units ("five hundred milliseconds" → `500 ms`) and file names ("package dot json"
//! → `package.json`).
//!
//! The pass runs inside the deterministic formatter, after the dictionary rules and names and
//! before capitalisation, so it works with formatting off and costs microseconds. Each rule
//! fires only on an unambiguous shape: an address must end in a known domain ending, a chord
//! must end in a key name, arithmetic must be the whole sentence. Anything else is left to the
//! formatting model.

mod arithmetic;
mod chord;
mod email;
mod files;
mod numbers;

use crate::vocabulary::Vocabulary;

pub(crate) use email::is_address;

/// Rewrites every spoken form in `text`.
pub(crate) fn rewrite(text: &str, vocabulary: &Vocabulary) -> String {
    let mut text = text.to_owned();
    let edits = email::edits(&text, vocabulary);
    apply(&mut text, edits);
    let edits = chord::edits(&text);
    apply(&mut text, edits);
    let edits = files::edits(&text);
    apply(&mut text, edits);
    let edits = arithmetic::edits(&text);
    apply(&mut text, edits);
    let edits = numbers::unit_edits(&text);
    apply(&mut text, edits);
    text
}

fn apply(text: &mut String, edits: Vec<Edit>) {
    if !edits.is_empty() {
        *text = splice(text, edits);
    }
}

/// One replacement: the bytes `start..end` of the text become `with`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) with: String,
}

/// Applies non-overlapping edits given in text order.
fn splice(text: &str, edits: Vec<Edit>) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut cursor = 0;
    for edit in edits {
        if edit.start < cursor {
            continue;
        }
        out.push_str(&text[cursor..edit.start]);
        out.push_str(&edit.with);
        cursor = edit.end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// A whitespace-separated word without the punctuation around it.
#[derive(Debug, Clone)]
pub(crate) struct Token<'a> {
    /// Byte range of `core` in the text.
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) core: &'a str,
    pub(crate) lower: String,
    /// Punctuation that followed the word ("," or "."), which ends a phrase.
    pub(crate) closed: bool,
}

fn is_leading(c: char) -> bool {
    matches!(c, '"' | '\'' | '(' | '[' | '{' | '“' | '‘')
}

fn is_trailing(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | ';' | ':' | '!' | '?' | '"' | '\'' | ')' | ']' | '}' | '”' | '’'
    )
}

pub(crate) fn tokens(text: &str) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut offset = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        let lead = word.len() - word.trim_start_matches(is_leading).len();
        let core = word[lead..].trim_end_matches(is_trailing);
        if !core.is_empty() {
            let start = offset + lead;
            out.push(Token {
                start,
                end: start + core.len(),
                core,
                lower: core.to_lowercase(),
                closed: lead + core.len() < word.len(),
            });
        }
        offset += piece.len();
    }
    out
}

/// Sentences as token ranges: a sentence ends at a token followed by `.`, `?` or `!`.
pub(crate) fn sentences(text: &str, tokens: &[Token<'_>]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut first = 0;
    for (index, token) in tokens.iter().enumerate() {
        let after = text[token.end..]
            .chars()
            .take_while(|c| !c.is_whitespace())
            .any(|c| matches!(c, '.' | '?' | '!'));
        if after || index + 1 == tokens.len() {
            out.push(first..index + 1);
            first = index + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spoken(text: &str) -> String {
        rewrite(text, &Vocabulary::default())
    }

    #[test]
    fn tokens_carry_their_core_without_punctuation() {
        let text = "Press (control), then ankit@sienzo.com.";
        let cores: Vec<&str> = tokens(text).iter().map(|t| t.core).collect();
        assert_eq!(cores, ["Press", "control", "then", "ankit@sienzo.com"]);
        let parts = tokens(text);
        assert!(parts[1].closed && !parts[0].closed);
        assert_eq!(&text[parts[3].start..parts[3].end], "ankit@sienzo.com");
    }

    #[test]
    fn sentences_split_on_end_punctuation() {
        let text = "Two plus two. Is it four? Yes";
        let found = sentences(text, &tokens(text));
        assert_eq!(found, vec![0..3, 3..6, 6..7]);
    }

    #[test]
    fn every_rule_runs_with_formatting_off() {
        assert_eq!(
            spoken("Press control plus alt plus delete, then two plus two equal to four."),
            "Press Ctrl+Alt+Delete, then two plus two equal to four."
        );
        assert_eq!(spoken("Two plus two equal to four."), "2 + 2 = 4");
        assert_eq!(
            spoken("Send it to priya dot sharma at gmail dot com"),
            "Send it to priya.sharma@gmail.com"
        );
    }

    #[test]
    fn ordinary_speech_is_left_alone() {
        for text in [
            "Let's meet at the office at five.",
            "I have control over all of it.",
            "We are at the rate of five percent growth.",
            "Option a is better than option b.",
            "Shift the meeting to Monday.",
            "Two people joined, one left.",
            "I'm at home and dot my i's.",
        ] {
            assert_eq!(spoken(text), text, "{text}");
        }
    }
}
