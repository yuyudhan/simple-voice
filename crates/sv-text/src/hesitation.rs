// FilePath: crates/sv-text/src/hesitation.rs
//! Hesitation sounds ("um", "uhh", "ahum", "hmm", "e-au") dropped from the raw transcript before
//! anything else sees it. This runs deterministically, so they disappear even when the LLM pass
//! is off, fails, or keeps them, and the model gets a cleaner transcript to format.
//!
//! Matching is by shape rather than by a word list: repeated letters collapse ("ummmm" → "um"),
//! so every elongation a recogniser writes is covered by one entry. Real words that share a
//! shape stay: "err" (the verb), "hum" (Hindi "we"), "mm" (millimetres), "uh-huh" (yes).

/// Hesitation shapes after lower-casing and collapsing runs of the same letter.
const SHAPES: [&str; 10] = [
    "um", "uhm", "uh", "ah", "ahm", "ahum", "eh", "er", "erm", "hm",
];

/// Longest collapsed shape in [`SHAPES`]; anything longer cannot match.
const MAX_SHAPE: usize = 4;

/// Real words whose collapsed shape is a hesitation.
const KEPT_WORDS: [&str; 1] = ["err"];

/// Removes hesitation tokens, with the punctuation the recogniser attached to them. A sentence
/// end carried by a hesitation that closes a line ("so that's it, um.") moves to the word
/// before it, so the sentence still ends. Lines left empty are dropped.
pub(crate) fn strip_hesitations(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split('\n') {
        let line_break = out.len();
        if !out.is_empty() {
            out.push('\n');
        }
        let line_start = out.len();
        strip_line(line, line_start, &mut out);
        if out.len() == line_start {
            out.truncate(line_break);
        }
    }
    out
}

/// True when the transcript holds no word other than hesitations, so there is nothing to paste.
pub fn only_hesitations(text: &str) -> bool {
    text.split_whitespace()
        .all(|word| is_hesitation(core(word)))
}

fn strip_line(line: &str, line_start: usize, out: &mut String) {
    let mut words = line.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let kept = core(word);
        if !is_hesitation(kept) {
            if out.len() > line_start {
                out.push(' ');
            }
            out.push_str(word);
            continue;
        }
        if words.peek().is_some() || out.len() == line_start {
            continue;
        }
        let marks = word.get(kept.len()..).unwrap_or_default();
        if let Some(end) = marks.chars().rev().find(|c| is_sentence_end(*c)) {
            while out.ends_with([',', ';', ':']) {
                out.pop();
            }
            if !out.ends_with(is_sentence_end) {
                out.push(end);
            }
        }
    }
}

/// The word without the punctuation a recogniser appends ("Um," "uh..." "um-").
fn core(word: &str) -> &str {
    word.trim_end_matches([',', '.', ';', ':', '?', '!', '-', '…'])
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '?' | '!')
}

/// A single hesitation shape, or a hyphenated run of hesitations and bare vowels ("e-au",
/// "uh-um"). A lone vowel is never one: "a" and "I" are words.
fn is_hesitation(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    if !word.contains('-') {
        return is_shape(word);
    }
    word.split('-')
        .all(|part| is_shape(part) || is_vowel_run(part))
}

fn is_shape(word: &str) -> bool {
    if KEPT_WORDS
        .iter()
        .any(|kept| word.eq_ignore_ascii_case(kept))
    {
        return false;
    }
    let mut shape = [0_u8; MAX_SHAPE];
    let mut len = 0;
    for byte in word.bytes() {
        if !byte.is_ascii_alphabetic() {
            return false;
        }
        let byte = byte.to_ascii_lowercase();
        if len > 0 && shape.get(len - 1) == Some(&byte) {
            continue;
        }
        let Some(slot) = shape.get_mut(len) else {
            return false;
        };
        *slot = byte;
        len += 1;
    }
    let collapsed = shape.get(..len).unwrap_or_default();
    SHAPES
        .iter()
        .any(|candidate| candidate.as_bytes() == collapsed)
}

fn is_vowel_run(part: &str) -> bool {
    !part.is_empty()
        && part
            .bytes()
            .all(|b| matches!(b.to_ascii_lowercase(), b'a' | b'e' | b'i' | b'o' | b'u'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_every_elongation_and_its_punctuation() {
        assert_eq!(
            strip_hesitations("Um, so ummmm the uhh build ahum works, hmm... fine eh"),
            "so the build works, fine"
        );
        assert_eq!(
            strip_hesitations("UHM we ship Ahh, today erm"),
            "we ship today"
        );
    }

    #[test]
    fn drops_hyphenated_hesitation_runs() {
        assert_eq!(strip_hesitations("e-au the uh-um plan"), "the plan");
    }

    #[test]
    fn keeps_words_that_share_a_shape() {
        let text = "hum log err on the side, 5 mm, uh-huh, a I- I think, um-brella, humm";
        assert_eq!(strip_hesitations(text), text);
    }

    #[test]
    fn a_closing_hesitation_hands_its_sentence_end_to_the_previous_word() {
        assert_eq!(strip_hesitations("that's it, um."), "that's it.");
        assert_eq!(strip_hesitations("ready uh?"), "ready?");
        assert_eq!(strip_hesitations("done. Um."), "done.");
        assert_eq!(strip_hesitations("wait um..."), "wait.");
    }

    #[test]
    fn a_line_of_only_hesitations_disappears() {
        assert_eq!(strip_hesitations("first\num, uh.\nsecond"), "first\nsecond");
        assert_eq!(strip_hesitations("um\nsecond"), "second");
        assert_eq!(strip_hesitations("uh... hmm"), "");
    }

    #[test]
    fn only_hesitations_means_nothing_was_said() {
        assert!(only_hesitations("  Um... uhh, hmm "));
        assert!(only_hesitations(""));
        assert!(!only_hesitations("um yes"));
        assert!(!only_hesitations("err"));
    }
}
