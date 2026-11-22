// FilePath: crates/sv-text/src/speculation/prompt.rs
//! The formatting request for one segment and the guards on its reply. The prompt is the whole
//! pass's prompt (same instructions, examples, preferred spellings and languages, so the model
//! prepared for one serves the other) with the segments before it in the user turn as read-only
//! context. The guards are the whole pass's, plus three for the seams: the reply must not repeat
//! the context, must keep most of the segment's own words, and must keep its sentences apart
//! unless the segment corrects itself or the reply is a list.

use sv_domain::Style;

use super::match_words;
use super::speculator::corrects_itself;
use crate::polish::{accept_polish, polish_prompt, PolishOutcome, PolishPrompt, PolishTarget};

const CONTEXT_OPEN: &str = "<context>";
const CONTEXT_CLOSE: &str = "</context>";
const CONTEXT_RULE: &str = "The context above was typed just before the transcript and is \
already formatted. Do not repeat or change it; format only the transcript, as its continuation.";
/// Words long enough to carry the content; shorter ones the model may rightly drop or merge.
const CONTENT_WORD_CHARS: usize = 4;
/// Share of the segment's content words the reply must keep. Formatting drops fillers and
/// false starts, which are mostly short words; a reply losing more than this lost content.
const MIN_KEPT_PERCENT: usize = 60;
/// Fewer lost content words than this are a rewording ("equal to" -> "equals"), not a loss.
const MIN_LOST_WORDS: usize = 2;
/// A run of this many context words in the reply, absent from the segment, is an echo.
const ECHO_WORDS: usize = 3;

/// The prompt for `text`, one segment, after the already formatted `context`.
pub fn polish_segment_prompt(
    context: &str,
    text: &str,
    terms: &[String],
    style: Style,
    target: PolishTarget,
    languages: &[String],
) -> PolishPrompt {
    let mut prompt = polish_prompt(text, terms, style, target, languages);
    let context = context.trim();
    if !context.is_empty() {
        prompt.user = format!(
            "{CONTEXT_OPEN}\n{context}\n{CONTEXT_CLOSE}\n{CONTEXT_RULE}\n{}",
            prompt.user
        );
    }
    prompt
}

/// Decides whether a model reply may replace the segment `input` formatted after `context`.
/// The segment ends a sentence when `input` does; a reply that lost its closing mark gets it
/// back, since the segment after it starts a new sentence.
pub fn accept_segment(context: &str, input: &str, output: &str, finished: bool) -> PolishOutcome {
    let outcome = accept_polish(input, output, finished);
    let PolishOutcome::Polished(reply) = outcome else {
        return outcome;
    };
    if echoes(context, input, &reply) {
        return PolishOutcome::Skipped("the reply repeated the context".to_owned());
    }
    if !keeps_content(input, &reply) {
        return PolishOutcome::Skipped("the reply dropped words".to_owned());
    }
    let reply = close_like(input, reply);
    if merges_sentences(input, &reply) {
        return PolishOutcome::Skipped("the reply merged sentences".to_owned());
    }
    PolishOutcome::Polished(reply)
}

/// Words that end a sentence.
fn sentence_ends(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.ends_with(['.', '?', '!']))
        .count()
}

/// Whether `reply` ran sentences of `input` together ("= 6. Ten" -> "= 6 10"), where the whole
/// pass, seeing them all, keeps them apart. Folding a self-correction into the sentence it
/// corrects, or turning sentences into list lines, may rightly lose sentence ends.
fn merges_sentences(input: &str, reply: &str) -> bool {
    sentence_ends(reply) < sentence_ends(input) && !reply.contains('\n') && !corrects_itself(input)
}

/// `reply`, ending with `input`'s sentence-ending mark when `input` has one and `reply`, not a
/// list, has none.
fn close_like(input: &str, mut reply: String) -> String {
    let ends = |text: &str| {
        text.trim_end()
            .chars()
            .next_back()
            .filter(|c| matches!(c, '.' | '?' | '!'))
    };
    if reply.contains('\n') {
        return reply;
    }
    if let (Some(mark), None) = (ends(input), ends(&reply)) {
        reply.truncate(reply.trim_end().len());
        reply.push(mark);
    }
    reply
}

/// Whether `reply` repeats the start or the end of `context` where `input` does not say it.
fn echoes(context: &str, input: &str, reply: &str) -> bool {
    let context = match_words(context);
    if context.len() < ECHO_WORDS {
        return false;
    }
    let input = match_words(input);
    let reply = match_words(reply);
    let ends = [
        context.get(..ECHO_WORDS),
        context.get(context.len() - ECHO_WORDS..),
    ];
    for run in ends.into_iter().flatten() {
        if contains_run(&reply, run) && !contains_run(&input, run) {
            return true;
        }
    }
    false
}

fn contains_run(words: &[String], run: &[String]) -> bool {
    words.windows(run.len()).any(|window| window == run)
}

/// Whether `reply` keeps the content words of `input`. A word kept with another ending
/// ("equal" as "equals") counts as kept.
fn keeps_content(input: &str, reply: &str) -> bool {
    let reply = match_words(reply);
    let content: Vec<String> = match_words(input)
        .into_iter()
        .filter(|word| word.chars().count() >= CONTENT_WORD_CHARS)
        .collect();
    let lost = content
        .iter()
        .filter(|word| {
            !reply.iter().any(|kept| {
                kept.starts_with(word.as_str())
                    || (kept.chars().count() >= CONTENT_WORD_CHARS
                        && word.starts_with(kept.as_str()))
            })
        })
        .count();
    lost < MIN_LOST_WORDS || (content.len() - lost) * 100 >= content.len() * MIN_KEPT_PERCENT
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTEXT: &str = "We should move the standup to 10 tomorrow.";
    const INPUT: &str = "Um the review can wait until Friday afternoon.";

    #[test]
    fn the_context_goes_before_the_transcript_in_the_user_turn() {
        let whole = polish_prompt(INPUT, &[], Style::Formal, PolishTarget::OnDevice, &[]);
        let segment = polish_segment_prompt(
            CONTEXT,
            INPUT,
            &[],
            Style::Formal,
            PolishTarget::OnDevice,
            &[],
        );
        assert_eq!(segment.system, whole.system);
        assert_eq!(segment.shots, whole.shots);
        assert!(segment.user.starts_with("<context>\nWe should move"));
        assert!(segment.user.ends_with(&whole.user));
        let alone = polish_segment_prompt("", INPUT, &[], Style::Formal, PolishTarget::Chat, &[]);
        assert_eq!(
            alone,
            polish_prompt(INPUT, &[], Style::Formal, PolishTarget::Chat, &[])
        );
    }

    #[test]
    fn a_clean_reply_is_accepted() {
        let reply = "The review can wait until Friday afternoon.";
        assert_eq!(
            accept_segment(CONTEXT, INPUT, reply, true),
            PolishOutcome::Polished(reply.to_owned())
        );
    }

    #[test]
    fn a_reply_repeating_the_context_is_refused() {
        let reply = format!("{CONTEXT} The review can wait until Friday afternoon.");
        assert_eq!(
            accept_segment(CONTEXT, INPUT, &reply, true),
            PolishOutcome::Skipped("the reply repeated the context".to_owned())
        );
    }

    #[test]
    fn context_words_the_segment_itself_says_are_no_echo() {
        let input = "We should move the standup again next week.";
        let reply = "We should move the standup again next week.";
        assert_eq!(
            accept_segment(CONTEXT, input, reply, true),
            PolishOutcome::Polished(reply.to_owned())
        );
    }

    #[test]
    fn a_reworded_word_is_no_loss() {
        let input = "Ten divided by 2 equal to is 5.";
        let reply = "Ten divided by 2 equals 5.";
        assert_eq!(
            accept_segment(CONTEXT, input, reply, true),
            PolishOutcome::Polished(reply.to_owned())
        );
    }

    #[test]
    fn a_reply_that_lost_the_closing_mark_gets_it_back() {
        assert_eq!(
            accept_segment(
                CONTEXT,
                "Can the review wait till Friday?",
                "Can the review wait until Friday",
                true
            ),
            PolishOutcome::Polished("Can the review wait until Friday?".to_owned())
        );
        assert_eq!(
            accept_segment(
                CONTEXT,
                "the review can wait till Friday",
                "The review can wait until Friday",
                true
            ),
            PolishOutcome::Polished("The review can wait until Friday".to_owned())
        );
    }

    #[test]
    fn a_reply_running_sentences_together_is_refused() {
        assert_eq!(
            accept_segment(
                CONTEXT,
                "2 + 2 + 2 = 6. 10 / 2 = 5. Set the timeout to 500 ms.",
                "2 + 2 + 2 = 6 10 / 2 = 5. Set the timeout to 500 ms.",
                true
            ),
            PolishOutcome::Skipped("the reply merged sentences".to_owned())
        );
    }

    #[test]
    fn a_folded_correction_or_a_list_may_lose_sentence_ends() {
        let corrected = "Send it on Monday. No wait, send it on Tuesday.";
        assert_eq!(
            accept_segment(CONTEXT, corrected, "Send it on Tuesday.", true),
            PolishOutcome::Polished("Send it on Tuesday.".to_owned())
        );
        let items = "Buy the milk today. Buy the eggs today. Buy the bread today.";
        let list = "- Buy the milk today\n- Buy the eggs today\n- Buy the bread today";
        assert_eq!(
            accept_segment(CONTEXT, items, list, true),
            PolishOutcome::Polished(list.to_owned())
        );
    }

    #[test]
    fn a_reply_that_lost_the_segment_is_refused() {
        assert_eq!(
            accept_segment(CONTEXT, INPUT, "The review.", true),
            PolishOutcome::Skipped("the reply dropped words".to_owned())
        );
    }

    #[test]
    fn the_whole_pass_guards_still_apply() {
        assert_eq!(
            accept_segment(CONTEXT, INPUT, "", true),
            PolishOutcome::Skipped("empty response".to_owned())
        );
        assert_eq!(
            accept_segment(CONTEXT, INPUT, "The review", false),
            PolishOutcome::Skipped("truncated".to_owned())
        );
    }
}
