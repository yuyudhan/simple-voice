// FilePath: crates/sv-text/src/snippets.rs
//! Deterministic snippet expansion. Triggers are matched like dictionary rules (case-insensitive,
//! on word frontiers, any whitespace run between their words) in one left-to-right pass where
//! the longest trigger at a position wins, so a payload is never scanned again and cannot expand
//! another snippet.
//!
//! The optional LLM formatting pass must not rewrite payloads, so expansion runs in two phases:
//! [`protect`] swaps every hit for a short opaque marker the model is told to copy
//! ([`MARKER_RULE`]), and [`Protected::restore`] puts the payloads back verbatim, refusing a reply
//! in which any marker went missing, was duplicated or altered.

use sv_domain::Snippet;

use crate::formatting::Pattern;

/// Appended to the formatting pass's system prompt when the transcript carries markers. Starts
/// with the "\n- " that separates the prompt's rules.
pub const MARKER_RULE: &str = "\n- Snippet markers: a token like [[S1]] stands for saved text \
the speaker inserted. Copy every marker exactly as written, once and in its place; never \
change, translate, split, merge or remove one.";

const MARKER_OPEN: &str = "[[S";
const MARKER_CLOSE: &str = "]]";

/// A transcript with its snippet hits swapped for markers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protected {
    /// What the formatting pass sees.
    text: String,
    /// The transcript with every payload in place, without the formatting pass.
    expanded: String,
    /// Payload of marker `[[S{n}]]` at index `n - 1`.
    payloads: Vec<String>,
    /// False when the transcript itself already contained marker-like text, so markers could
    /// not be told apart: `text` then holds the expanded text and no reply can be restored.
    marked: bool,
}

impl Protected {
    /// The text to send to the formatting pass.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The transcript with every payload inserted; the fallback when no polished reply can be
    /// restored.
    pub fn expanded(&self) -> &str {
        &self.expanded
    }

    /// How many snippets were expanded.
    pub fn hits(&self) -> usize {
        self.payloads.len()
    }

    /// The polished reply with every marker replaced by its payload, or `None` when a marker is
    /// missing, appears more than once, was altered or was invented, in which case the caller
    /// uses [`Protected::expanded`] instead.
    pub fn restore(&self, polished: &str) -> Option<String> {
        if self.payloads.is_empty() {
            return Some(polished.to_owned());
        }
        if !self.marked {
            return None;
        }
        let payload_chars: usize = self.payloads.iter().map(String::len).sum();
        let mut out = String::with_capacity(polished.len() + payload_chars);
        let mut seen = vec![false; self.payloads.len()];
        let mut rest = polished;
        while let Some(start) = rest.find(MARKER_OPEN) {
            let (before, from) = rest.split_at(start);
            out.push_str(before);
            let after = from.strip_prefix(MARKER_OPEN)?;
            let digits_len = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            let (digits, tail) = after.split_at(digits_len);
            if digits.starts_with('0') {
                return None;
            }
            let slot = digits.parse::<usize>().ok()?.checked_sub(1)?;
            let tail = tail.strip_prefix(MARKER_CLOSE)?;
            let payload = self.payloads.get(slot)?;
            if std::mem::replace(seen.get_mut(slot)?, true) {
                return None;
            }
            out.push_str(payload);
            rest = tail;
        }
        out.push_str(rest);
        seen.iter().all(|&restored| restored).then_some(out)
    }
}

/// Finds every trigger in `text` (the deterministically formatted transcript) and swaps it for a
/// marker. Punctuation and spacing around a trigger stay where they are.
pub fn protect(text: &str, snippets: &[Snippet]) -> Protected {
    let patterns: Vec<(Pattern, &str)> = snippets
        .iter()
        .filter_map(|snippet| {
            Pattern::compile(&snippet.trigger).map(|pattern| (pattern, snippet.text.as_str()))
        })
        .collect();
    let chars: Vec<char> = text.chars().collect();
    let mut marked = String::with_capacity(text.len());
    let mut expanded = String::with_capacity(text.len());
    let mut payloads: Vec<String> = Vec::new();
    let mut at = 0;
    while let Some(&c) = chars.get(at) {
        match longest_match(&patterns, &chars, at) {
            Some((end, payload)) => {
                payloads.push(payload.to_owned());
                marked.push_str(MARKER_OPEN);
                marked.push_str(&payloads.len().to_string());
                marked.push_str(MARKER_CLOSE);
                expanded.push_str(payload);
                at = end;
            }
            None => {
                marked.push(c);
                expanded.push(c);
                at += 1;
            }
        }
    }
    let collides = !payloads.is_empty() && text.contains(MARKER_OPEN);
    Protected {
        text: if collides { expanded.clone() } else { marked },
        expanded,
        payloads,
        marked: !collides,
    }
}

/// Expands every trigger in `text` without a formatting pass.
pub fn expand(text: &str, snippets: &[Snippet]) -> String {
    protect(text, snippets).expanded
}

/// End and payload of the longest trigger matching at `at`; the earliest listed wins a tie.
fn longest_match<'a>(
    patterns: &[(Pattern, &'a str)],
    chars: &[char],
    at: usize,
) -> Option<(usize, &'a str)> {
    let mut best: Option<(usize, &'a str)> = None;
    for (pattern, payload) in patterns {
        let Some(end) = pattern.match_at(chars, at) else {
            continue;
        };
        if end > at && best.is_none_or(|(best_end, _)| end > best_end) {
            best = Some((end, *payload));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippets(pairs: &[(&str, &str)]) -> Vec<Snippet> {
        pairs
            .iter()
            .zip(1..)
            .map(|((trigger, text), id)| Snippet {
                id,
                trigger: (*trigger).to_owned(),
                text: (*text).to_owned(),
                created_at: 0,
                updated_at: 0,
            })
            .collect()
    }

    #[test]
    fn triggers_match_whole_words_only() {
        let s = snippets(&[("address", "221B Baker Street")]);
        assert_eq!(expand("open the addressbook", &s), "open the addressbook");
        assert_eq!(expand("my emailaddress", &s), "my emailaddress");
        assert_eq!(
            expand("send (address), thanks", &s),
            "send (221B Baker Street), thanks"
        );
        assert_eq!(protect("open the addressbook", &s).hits(), 0);
    }

    #[test]
    fn triggers_match_ignoring_case_and_across_whitespace_runs() {
        let s = snippets(&[("my  sign off", "Cheers")]);
        assert_eq!(expand("MY SIGN\nOff.", &s), "Cheers.");
        assert_eq!(expand("My   sign off", &s), "Cheers");
    }

    #[test]
    fn longest_trigger_wins_and_the_pass_runs_left_to_right() {
        let s = snippets(&[("sig", "S"), ("sig long", "LONG"), ("long form", "FORM")]);
        assert_eq!(expand("sig long now and sig", &s), "LONG now and S");
        // "sig long" starts first, so the overlapping "long form" never gets its turn.
        assert_eq!(expand("sig long form", &s), "LONG form");
        let protected = protect("sig long now and sig", &s);
        assert_eq!(protected.text(), "[[S1]] now and [[S2]]");
        assert_eq!(protected.hits(), 2);
    }

    #[test]
    fn payloads_are_never_expanded_again() {
        let s = snippets(&[("alpha", "beta alpha"), ("beta", "gamma")]);
        assert_eq!(expand("alpha then beta", &s), "beta alpha then gamma");
    }

    #[test]
    fn multiline_payload_is_inserted_verbatim_before_the_punctuation() {
        let address = "221B Baker Street\n  London NW1 6XE\n";
        let s = snippets(&[("home address", address)]);
        assert_eq!(
            expand("Ship it to home address, please.", &s),
            format!("Ship it to {address}, please.")
        );
    }

    #[test]
    fn restore_puts_payloads_back_into_the_polished_reply() {
        let s = snippets(&[("my email", "me@example.com"), ("sig", "Best,\nAnkur")]);
        let protected = protect("um send it to my email ok sig", &s);
        assert_eq!(protected.text(), "um send it to [[S1]] ok [[S2]]");
        let polished = "Send it to [[S1]].\n\n[[S2]]";
        assert_eq!(
            protected.restore(polished).as_deref(),
            Some("Send it to me@example.com.\n\nBest,\nAnkur")
        );
        assert_eq!(
            protected.expanded(),
            "um send it to me@example.com ok Best,\nAnkur"
        );
    }

    #[test]
    fn restore_refuses_missing_duplicated_altered_or_invented_markers() {
        let s = snippets(&[("one", "1st"), ("two", "2nd")]);
        let protected = protect("one and two", &s);
        assert_eq!(protected.text(), "[[S1]] and [[S2]]");
        for reply in [
            "[[S1]] and",
            "[[S1]] and [[S2]] [[S2]]",
            "[[s1]] and [[S2]]",
            "[[S 1]] and [[S2]]",
            "[[S01]] and [[S2]]",
            "[[S1]] and [[S2]] [[S3]]",
            "[[S1] and [[S2]]",
        ] {
            assert_eq!(protected.restore(reply), None, "{reply}");
        }
        assert_eq!(
            protected.restore("[[S2]] and [[S1]]").as_deref(),
            Some("2nd and 1st")
        );
    }

    #[test]
    fn a_payload_that_looks_like_a_marker_is_not_rescanned() {
        let s = snippets(&[("one", "[[S2]]"), ("two", "second")]);
        let protected = protect("one two", &s);
        assert_eq!(
            protected.restore("[[S1]] [[S2]]").as_deref(),
            Some("[[S2]] second")
        );
    }

    #[test]
    fn transcript_with_marker_text_is_expanded_but_never_restored() {
        let s = snippets(&[("sig", "Ankur")]);
        let protected = protect("literal [[S1]] then sig", &s);
        assert_eq!(protected.text(), "literal [[S1]] then Ankur");
        assert_eq!(protected.expanded(), "literal [[S1]] then Ankur");
        assert_eq!(protected.restore("Literal [[S1]] then Ankur"), None);
    }

    #[test]
    fn without_hits_the_reply_passes_through() {
        let s = snippets(&[("sig", "Ankur")]);
        let protected = protect("nothing here", &s);
        assert_eq!(protected.text(), "nothing here");
        assert_eq!(
            protected.restore("Nothing here.").as_deref(),
            Some("Nothing here.")
        );
        assert_eq!(expand("nothing here", &[]), "nothing here");
    }
}
