// FilePath: crates/sv-text/src/speculation/plan.rs
//! What the stop reuses: the formatted segments whose input is, token for token (punctuation and
//! capitals included), the next stretch of the whole recording's formatted transcript. The first
//! segment that differs ends the reuse, and everything after it is the tail the model formats
//! after the stop. The pasted words always come from the whole transcript: a reused segment is
//! the model's formatting of exactly the whole text's own stretch, so a draft that punctuated a
//! sentence differently never carries its punctuation into the paste, and the tail is a slice
//! of the whole text, so no word is repeated or lost at a seam.

use super::match_words;
use super::speculator::{context_of, Formatted};

/// Words that announce list structure. When two or more parts of a stop's text carry one, the
/// list may span parts, and only a pass over the whole text can lay it out.
const LIST_CUES: &[&str] = &[
    "first", "firstly", "second", "secondly", "third", "thirdly", "bullet", "bullets", "list",
    "lastly", "finally",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The reused segments' formatted text, in order.
    pub reused: Vec<String>,
    /// Words of the whole text the reused segments cover.
    pub reused_words: usize,
    /// The whole text after the reused words, still to be formatted.
    pub tail: String,
    /// The last reused outputs: the tail's context.
    pub context: String,
    /// Reused segments and tail that carry a list cue.
    pub list_parts: usize,
}

impl Plan {
    /// Whether the layout may span parts, so the whole text must be formatted in one pass.
    pub fn spans_a_list(&self) -> bool {
        self.list_parts > 1
    }
}

/// Matches `formatted` (in order) against `whole`, the deterministic formatter's output for the
/// whole transcript.
pub fn plan(whole: &str, formatted: &[Formatted]) -> Plan {
    // Each token of `whole` with the byte offset where it ends.
    let tokens = tokens(whole);
    let mut matched = 0;
    let mut cut = 0;
    let mut reused = Vec::new();
    let mut list_parts = 0;
    for segment in formatted {
        let input: Vec<&str> = segment.input.split_whitespace().collect();
        let Some(next) = tokens.get(matched..matched + input.len()) else {
            break;
        };
        let Some(mark) = same_stretch(&input, next) else {
            break;
        };
        matched += input.len();
        if let Some((_, end)) = next.last() {
            cut = *end;
        }
        if !segment.output.trim().is_empty() {
            list_parts += usize::from(has_list_cue(&segment.input));
            reused.push(closed_with(&segment.output, mark));
        }
    }
    let tail = whole.get(cut..).unwrap_or_default().trim().to_owned();
    list_parts += usize::from(has_list_cue(&tail));
    let outputs: Vec<&str> = reused.iter().map(String::as_str).collect();
    Plan {
        context: context_of(&outputs),
        reused,
        reused_words: match_words(whole.get(..cut).unwrap_or_default()).len(),
        tail,
        list_parts,
    }
}

/// Whether the segment `input` is the stretch `whole` of the whole text, token for token.
/// Formatted alone, a segment may lack the sentence mark its last word carries within the whole
/// text (the formatter's end-of-text rules), which is then returned for its output to carry.
fn same_stretch(input: &[&str], whole: &[(&str, usize)]) -> Option<Option<char>> {
    let (last, rest) = input.split_last()?;
    let (whole_last, whole_rest) = whole.split_last()?;
    if !rest.iter().eq(whole_rest.iter().map(|(token, _)| token)) {
        return None;
    }
    if last == &whole_last.0 {
        return Some(None);
    }
    let mark = whole_last.0.strip_prefix(last)?;
    let mut chars = mark.chars();
    match (chars.next(), chars.next()) {
        (Some(mark @ ('.' | '?' | '!')), None) => Some(Some(mark)),
        _ => None,
    }
}

/// `output`, ending with `mark` when it has no sentence mark of its own.
fn closed_with(output: &str, mark: Option<char>) -> String {
    let output = output.trim_end();
    match mark {
        Some(mark) if !output.ends_with(['.', '?', '!']) && !output.contains('\n') => {
            format!("{output}{mark}")
        }
        _ => output.to_owned(),
    }
}

/// Joins formatted parts with a space, or a line break where either side is a list line.
pub fn join(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
    {
        if !out.is_empty() {
            let list = out.lines().last().is_some_and(is_list_line)
                || part.lines().next().is_some_and(is_list_line);
            out.push(if list { '\n' } else { ' ' });
        }
        out.push_str(part);
    }
    out
}

/// Each whitespace token of `text` with the byte offset where it ends.
fn tokens(text: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    for (at, c) in text.char_indices().chain([(text.len(), ' ')]) {
        if !c.is_whitespace() {
            start = start.or(Some(at));
        } else if let Some(from) = start.take() {
            out.push((text.get(from..at).unwrap_or_default(), at));
        }
    }
    out
}

fn has_list_cue(text: &str) -> bool {
    match_words(text)
        .iter()
        .any(|word| LIST_CUES.contains(&word.as_str()))
}

/// A bullet ("- ", "• ") or numbered ("1. ") line.
fn is_list_line(line: &str) -> bool {
    let line = line.trim_start();
    if line.starts_with("- ") || line.starts_with("• ") {
        return true;
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && line
            .get(digits..)
            .is_some_and(|rest| rest.starts_with(". "))
}

#[cfg(test)]
mod tests {
    use sv_domain::Style;

    use super::*;
    use crate::{format, Vocabulary};

    fn segment(input: &str, output: &str) -> Formatted {
        Formatted {
            input: input.to_owned(),
            output: output.to_owned(),
        }
    }

    const WHOLE: &str = "We should move the standup to ten. The review can wait. Thanks all.";

    #[test]
    fn an_exact_match_is_reused_and_the_rest_is_the_tail() {
        let plan = plan(
            WHOLE,
            &[segment(
                "We should move the standup to ten.",
                "We should move the standup to 10.",
            )],
        );
        assert_eq!(plan.reused, ["We should move the standup to 10."]);
        assert_eq!(plan.reused_words, 7);
        assert_eq!(plan.tail, "The review can wait. Thanks all.");
        assert_eq!(plan.context, "We should move the standup to 10.");
    }

    #[test]
    fn a_segment_punctuated_differently_is_not_reused() {
        let segments = [segment("We should move the standup, to ten.", "A.")];
        let plan = plan(WHOLE, &segments);
        assert!(plan.reused.is_empty());
        assert_eq!(plan.tail, WHOLE);
    }

    #[test]
    fn a_segment_missing_only_its_closing_mark_is_reused_with_it() {
        let segments = [segment(
            "We should move the standup to ten",
            "We should move the standup to 10",
        )];
        let plan = plan(WHOLE, &segments);
        assert_eq!(plan.reused, ["We should move the standup to 10."]);
        assert_eq!(plan.tail, "The review can wait. Thanks all.");
    }

    #[test]
    fn a_revised_word_ends_the_reuse() {
        let segments = [
            segment("We should move the standup to ten.", "A."),
            segment("The review can't wait.", "B."),
            segment("Thanks all.", "C."),
        ];
        let plan = plan(WHOLE, &segments);
        assert_eq!(plan.reused, ["A."]);
        assert_eq!(plan.tail, "The review can wait. Thanks all.");
    }

    #[test]
    fn a_segment_after_a_miss_is_not_reused_even_when_it_matches() {
        let segments = [
            segment("We should move the stand up to ten.", "A."),
            segment("The review can wait.", "B."),
        ];
        let plan = plan(WHOLE, &segments);
        assert!(plan.reused.is_empty());
        assert_eq!(plan.reused_words, 0);
        assert_eq!(plan.tail, WHOLE);
    }

    #[test]
    fn a_list_cue_in_two_parts_spans_a_list() {
        let segments = [segment("First we fix the login page today.", "A.")];
        let plan = plan(
            "First we fix the login page today. Second the signup flow.",
            &segments,
        );
        assert!(plan.spans_a_list());
        let plan = plan_one("We fix the login page today. Second the signup flow.");
        assert!(!plan.spans_a_list());
    }

    fn plan_one(whole: &str) -> Plan {
        plan(whole, &[segment("We fix the login page today.", "A.")])
    }

    #[test]
    fn join_breaks_lines_around_lists() {
        assert_eq!(join(&["One.", "", "Two."]), "One. Two.");
        assert_eq!(
            join(&["Items:\n- a\n- b", "Next."]),
            "Items:\n- a\n- b\nNext."
        );
        assert_eq!(join(&["Steps:", "1. Build"]), "Steps:\n1. Build");
    }

    /// Every sentence of the eval corpora, formatted one at a time the way the worker formats a
    /// segment, then reused at the stop with an identity model: the joined result carries the
    /// whole text's words exactly once and in order, whichever segment the reuse stops at.
    #[test]
    fn seams_never_drop_or_repeat_a_word() {
        let mut sentences = Vec::new();
        for corpus in [
            include_str!("../../../../scripts/eval/say-corpus.tsv"),
            include_str!("../../../../scripts/eval/spoken-corpus.tsv"),
        ] {
            for line in corpus.lines().filter(|line| !line.starts_with('#')) {
                if let Some(said) = line.split('\t').nth(1) {
                    let said = said.trim().trim_end_matches(['.', '?', '!']);
                    sentences.push(format!("{said}."));
                }
            }
        }
        assert!(sentences.len() > 60);
        let vocabulary = Vocabulary::default();
        let formatted = |text: &str| format(text, &vocabulary, Style::Formal).text;
        let whole = formatted(&sentences.join(" "));
        let segments: Vec<Formatted> = sentences
            .iter()
            .map(|sentence| {
                let input = formatted(sentence);
                segment(&input, &input)
            })
            .collect();
        for reused in 0..=segments.len() {
            let plan = plan(&whole, segments.get(..reused).unwrap_or_default());
            let mut parts: Vec<&str> = plan.reused.iter().map(String::as_str).collect();
            parts.push(&plan.tail);
            assert_eq!(
                match_words(&join(&parts)),
                match_words(&whole),
                "reuse {reused}"
            );
        }
        let all = plan(&whole, &segments);
        let missed = segments.get(all.reused.len()).map(|s| s.input.clone());
        let at = all.tail.get(..120).unwrap_or(&all.tail).to_owned();
        assert_eq!(all.reused.len(), segments.len(), "{missed:?} vs {at:?}");
        assert_eq!(all.tail, "");
    }
}
