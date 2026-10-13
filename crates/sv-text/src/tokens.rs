// FilePath: crates/sv-text/src/tokens.rs
//! Upper-bound estimate of how many Whisper tokens a prompt costs, without shipping the 800 KB
//! tokenizer. Whisper keeps at most 223 prompt tokens (half its 448-token text context, minus
//! one) and silently drops the oldest ones, which are the language lead and the user's most
//! important words, so the prompt has to be measured in tokens and not only in characters.
//!
//! Calibrated against Whisper's multilingual tokenizer on the built-in pack words: the estimate
//! is 1.15 times the real count over the whole list, a single term is never more than one token
//! under, names and romanised Hindi come out exact, and English prose comes out high.

/// Estimated tokens of `text`: each run of ASCII letters is split into case segments ("Argo",
/// "CD"); a lowercase or capitalised segment costs one token per three letters, an all-caps one
/// one per two; every digit and every other ASCII symbol costs one, whitespace none (it merges
/// into the next token), and any other character two.
pub(crate) fn estimate_tokens(text: &str) -> usize {
    let mut total = 0;
    let mut letters = String::new();
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            letters.push(c);
            continue;
        }
        total += letter_tokens(&letters);
        letters.clear();
        total += if c.is_whitespace() {
            0
        } else if c.is_ascii() {
            1
        } else {
            2
        };
    }
    total + letter_tokens(&letters)
}

/// Tokens of one run of ASCII letters. An uppercase run directly before a lowercase letter
/// gives its last capital to the word that follows ("SQLite" is "SQ" + "Lite").
fn letter_tokens(run: &str) -> usize {
    let bytes = run.as_bytes();
    let mut total = 0;
    let mut at = 0;
    while at < bytes.len() {
        let upper = bytes[at..]
            .iter()
            .take_while(|b| b.is_ascii_uppercase())
            .count();
        let next_is_lower = bytes.get(at + upper).is_some_and(u8::is_ascii_lowercase);
        if upper > 1 || (upper == 1 && !next_is_lower) {
            let acronym = if next_is_lower { upper - 1 } else { upper };
            total += acronym.div_ceil(2);
            at += acronym;
            continue;
        }
        let word = upper
            + bytes[at + upper..]
                .iter()
                .take_while(|b| b.is_ascii_lowercase())
                .count();
        total += word.div_ceil(3);
        at += word;
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_split_on_case_and_acronyms_cost_more_per_letter() {
        // "Argo" 2 + "CD" 1.
        assert_eq!(estimate_tokens("ArgoCD"), 3);
        // "SQ" 1 + "Lite" 2: the capital before lowercase starts the next segment.
        assert_eq!(estimate_tokens("SQLite"), 3);
        // "MEDDPICC" 4.
        assert_eq!(estimate_tokens("MEDDPICC"), 4);
        // "kubectl" 3.
        assert_eq!(estimate_tokens("kubectl"), 3);
    }

    #[test]
    fn separators_digits_and_symbols_cost_one_and_whitespace_none() {
        // "," 1 + "Node" 2 + "." 1 + "js" 1 + "EC" 1 + "2" 1.
        assert_eq!(estimate_tokens(", Node.js EC2"), 7);
        assert_eq!(estimate_tokens("  \n"), 0);
    }

    #[test]
    fn characters_outside_ascii_cost_two() {
        assert_eq!(estimate_tokens("कोड"), 6);
        assert_eq!(estimate_tokens("é"), 2);
    }
}
