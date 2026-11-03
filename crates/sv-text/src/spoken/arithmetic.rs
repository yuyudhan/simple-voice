// FilePath: crates/sv-text/src/spoken/arithmetic.rs
//! Spoken arithmetic, only when a whole sentence is numbers and operators with at least two
//! numbers: "two plus two plus two equal to six" → `2 + 2 + 2 = 6`, "ten divided by two equals
//! five" → `10 / 2 = 5`. A sentence with any other word ("two people joined") is prose.

use super::numbers::number;
use super::{sentences, tokens, Edit, Token};

/// The operator starting at `tokens[at]`, as written, and how many words it takes.
fn operator(tokens: &[Token<'_>], at: usize) -> Option<(&'static str, usize)> {
    let word = |offset: usize| tokens.get(at + offset).map(|t| t.lower.as_str());
    let found = match (word(0)?, word(1), word(2)) {
        ("is", Some("equal"), Some("to")) => ("=", 3),
        ("equal" | "equals", Some("to"), _) => ("=", 2),
        ("divided", Some("by"), _) => ("/", 2),
        ("multiplied", Some("by"), _) => ("×", 2),
        ("equals" | "equal" | "=" | "is", _, _) => ("=", 1),
        ("plus" | "+", _, _) => ("+", 1),
        ("minus" | "-", _, _) => ("-", 1),
        ("times" | "x" | "×" | "*", _, _) => ("×", 1),
        ("over" | "/", _, _) => ("/", 1),
        _ => return None,
    };
    // An operator's words belong together: "equal, to" is not one.
    let split = tokens[at..at + found.1 - 1].iter().any(|t| t.closed);
    (!split).then_some(found)
}

/// The sentence `tokens[range]` written as arithmetic, when it is nothing else.
fn expression(tokens: &[Token<'_>], range: std::ops::Range<usize>) -> Option<String> {
    let sentence = &tokens[..range.end];
    let mut out = String::new();
    let mut at = range.start;
    let mut numbers = 0;
    let mut equals = 0;
    loop {
        let (value, used) = number(sentence, at)?;
        out.push_str(&value);
        numbers += 1;
        at += used;
        if at == range.end {
            break;
        }
        if tokens[at - 1].closed {
            return None;
        }
        let (symbol, used) = operator(sentence, at)?;
        equals += usize::from(symbol == "=");
        out.push(' ');
        out.push_str(symbol);
        out.push(' ');
        at += used;
        if at == range.end {
            return None;
        }
    }
    (numbers >= 2 && equals <= 1).then_some(out)
}

pub(crate) fn edits(text: &str) -> Vec<Edit> {
    let tokens = tokens(text);
    let mut edits = Vec::new();
    for range in sentences(text, &tokens) {
        let (Some(first), Some(last)) = (range.clone().next(), range.clone().next_back()) else {
            continue;
        };
        let Some(written) = expression(&tokens, range) else {
            continue;
        };
        // A sum that is the whole dictation stands on its own; a closing period would read as
        // part of it. Inside longer text the period still ends the sentence.
        let mut end = tokens[last].end;
        if text[end..].trim_end() == "." {
            end = text.len();
        }
        if text[tokens[first].start..end] != written {
            edits.push(Edit {
                start: tokens[first].start,
                end,
                with: written,
            });
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn math(text: &str) -> String {
        super::super::splice(text, edits(text))
    }

    #[test]
    fn whole_sentences_of_arithmetic_become_symbols() {
        for (said, written) in [
            ("Two plus two plus two equal to six.", "2 + 2 + 2 = 6"),
            ("2 plus 2 plus 2 equal to 6.", "2 + 2 + 2 = 6"),
            ("Ten divided by two equals five.", "10 / 2 = 5"),
            ("10 divided by 2 equals 5", "10 / 2 = 5"),
            ("three times four is twelve", "3 × 4 = 12"),
            ("twenty five minus five", "25 - 5"),
            (
                "Done. Two plus two equals four. Next",
                "Done. 2 + 2 = 4. Next",
            ),
        ] {
            assert_eq!(math(said), written, "{said}");
        }
    }

    #[test]
    fn sentences_with_other_words_stay() {
        for text in [
            "Two people joined, one left.",
            "We need two plus points.",
            "It is two.",
            "Five.",
            "one equals one equals one",
            "Add two plus two to the list.",
            "2 + 2 = 4",
        ] {
            assert_eq!(math(text), text, "{text}");
        }
    }
}
