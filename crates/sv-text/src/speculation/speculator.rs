// FilePath: crates/sv-text/src/speculation/speculator.rs
//! Which words of the recording so far have settled, and which closed segments of them are
//! formatted next.
//!
//! The words come from drafts: the recording so far, transcribed again every few seconds with
//! the whole pass's model and options ([`draft_due`]). A word has settled once it sat unchanged at
//! the same place in two consecutive different drafts (the LocalAgreement-2 policy of
//! whisper-streaming), or once the newest draft has [`HOLD_BACK_WORDS`] more after it: each draft
//! hears every word with the whole recording before it, as the stop will, and only its end, cut
//! mid-sentence, keeps changing until more audio follows it.
//! A segment closes at a sentence end once two more settled words prove the sentence
//! ended, and once it holds at least [`MIN_SEGMENT_WORDS`]; a following sentence that opens with
//! a self-correction or list cue ("no wait", "I mean", "first") joins it, so the model sees the
//! correction with what it corrects. A segment whose words a later draft revises is dropped with
//! every segment after it; nothing was pasted, so nothing has to be undone. A segment whose
//! reply the guards refused closes again together with the next sentence.

use super::match_words;

/// The shortest audio between two drafts.
const DRAFT_INTERVAL_MS: u64 = 2_000;
/// Drafts may keep the speech model busy at most one part in this many of the recording time.
const DRAFT_DUTY: u64 = 5;
/// Words at the end of the newest draft that wait for the next one.
const HOLD_BACK_WORDS: usize = 4;

/// Whether the recording, now `audio_ms` long, is due a new draft: the last one covered
/// `drafted_ms` and took `cost_ms`. Drafts grow with the recording, so the interval grows with
/// their cost.
pub fn draft_due(audio_ms: u64, drafted_ms: u64, cost_ms: u64) -> bool {
    let interval = DRAFT_INTERVAL_MS.max(cost_ms.saturating_mul(DRAFT_DUTY));
    audio_ms >= drafted_ms.saturating_add(interval)
}

/// Shorter sentences wait and close together with the next one: the model's fixed cost per
/// request makes one-sentence requests slower in total, and a short sentence alone gives it
/// little to go on.
pub const MIN_SEGMENT_WORDS: usize = 8;
/// Settled words after a sentence end before it counts as ended: enough to see a two-word cue.
const FOLLOWING_WORDS: usize = 2;
/// Sentence openings that rework the sentence before them, as lowercase match words: the
/// self-corrections, then the list cues.
const HOLD_CUES: &[&[&str]] = &[
    &["no", "wait"],
    &["wait"],
    &["i", "mean"],
    &["scratch", "that"],
    &["actually"],
    &["sorry"],
    &["correction"],
    &["rather"],
    &["instead"],
    &["first"],
    &["firstly"],
    &["second"],
    &["secondly"],
    &["third"],
    &["next"],
    &["then"],
];
/// How many of [`HOLD_CUES`] are self-corrections.
const CORRECTION_CUES: usize = 9;

/// Whether a sentence in `text` (after the first) opens with a self-correction: the model may
/// then rightly fold it into the sentence it corrects.
pub(super) fn corrects_itself(text: &str) -> bool {
    let corrections = HOLD_CUES.get(..CORRECTION_CUES).unwrap_or_default();
    let words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
    (1..words.len()).any(|index| {
        words
            .get(index - 1)
            .is_some_and(|word| word.ends_with(['.', '?', '!']))
            && opens_with(words.get(index..).unwrap_or_default(), corrections)
    })
}

/// At most this many formatted segments, and words, go before a request as its context.
const CONTEXT_SEGMENTS: usize = 3;
const CONTEXT_MAX_WORDS: usize = 120;

/// A segment the model is asked to format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    /// The formatted segments before it, read-only for the model.
    pub context: String,
    /// The deterministic formatter's output for the segment's words.
    pub input: String,
}

/// A segment with the model's accepted reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reply {
    Pending(u64),
    Done(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    /// Index of its first word in the draft.
    start: usize,
    /// The draft words it covers, as the draft wrote them.
    heard: Vec<String>,
    input: String,
    reply: Reply,
}

impl Segment {
    fn end(&self) -> usize {
        self.start + self.heard.len()
    }
}

/// The settled words of one recording's drafts and its segments.
#[derive(Debug, Default)]
pub struct Speculator {
    previous: Vec<String>,
    current: Vec<String>,
    /// Leading words of `current` that have settled.
    settled: usize,
    segments: Vec<Segment>,
    next_id: u64,
    /// Start and end of the last segment whose reply was refused: the next segment from that
    /// start runs past that end, so its words go again together with the next sentence.
    refused: Option<(usize, usize)>,
    /// Segments dropped because a draft revised their words.
    revised: usize,
}

impl Speculator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the newest draft of the recording so far.
    pub fn observe(&mut self, text: &str) {
        let words: Vec<String> = text.split_whitespace().map(str::to_owned).collect();
        if words != self.current {
            self.previous = std::mem::replace(&mut self.current, words);
        }
        let agreed = self
            .previous
            .iter()
            .zip(&self.current)
            .take_while(|(before, now)| before == now)
            .count();
        self.settled = agreed.max(self.current.len().saturating_sub(HOLD_BACK_WORDS));
        let current = &self.current;
        let revised = self
            .segments
            .iter()
            .position(|segment| current.get(segment.start..segment.end()) != Some(&segment.heard));
        if let Some(index) = revised {
            self.revised += self.segments.len() - index;
            self.segments.truncate(index);
        }
    }

    /// The next closed segment to format, with `format` (the deterministic formatter) applied
    /// to its words; `None` while a request is out or when no segment has closed yet. Requests
    /// go one at a time, so each one's context is already formatted.
    pub fn next_request(&mut self, format: impl Fn(&str) -> String) -> Option<Request> {
        loop {
            if self
                .segments
                .iter()
                .any(|segment| !matches!(segment.reply, Reply::Done(_)))
            {
                return None;
            }
            let start = self.segments.last().map_or(0, Segment::end);
            let end = self.closed_end(start)?;
            let heard = self.current.get(start..=end)?.to_vec();
            let input = format(&heard.join(" "));
            let id = self.next_id;
            self.next_id += 1;
            // Hesitations only: nothing to format, and nothing to match at the stop.
            let empty = match_words(&input).is_empty();
            let reply = if empty {
                Reply::Done(String::new())
            } else {
                Reply::Pending(id)
            };
            let context = self.context();
            self.segments.push(Segment {
                start,
                heard,
                input: input.clone(),
                reply,
            });
            if !empty {
                return Some(Request { id, context, input });
            }
        }
    }

    /// Records the reply to request `id`: the accepted text, or `None` when the guards refused
    /// it, in which case its words close again with the next sentence. A request whose segment
    /// was revised meanwhile is ignored.
    pub fn complete(&mut self, id: u64, output: Option<String>) {
        let Some(index) = self
            .segments
            .iter()
            .position(|segment| segment.reply == Reply::Pending(id))
        else {
            return;
        };
        match output {
            Some(output) => {
                if let Some(segment) = self.segments.get_mut(index) {
                    segment.reply = Reply::Done(output);
                }
            }
            None => {
                self.refused = self
                    .segments
                    .get(index)
                    .map(|segment| (segment.start, segment.end()));
                self.segments.truncate(index);
            }
        }
    }

    /// The formatted segments in order, up to the first one without an accepted reply.
    pub fn formatted(&self) -> Vec<Formatted> {
        self.segments
            .iter()
            .map_while(|segment| match &segment.reply {
                Reply::Done(output) => Some(Formatted {
                    input: segment.input.clone(),
                    output: output.clone(),
                }),
                Reply::Pending(_) => None,
            })
            .collect()
    }

    /// Segments dropped because a draft revised their words.
    pub fn revised(&self) -> usize {
        self.revised
    }

    /// Index of the last word of the first segment that closed from `start`.
    fn closed_end(&self, start: usize) -> Option<usize> {
        let first = match self.refused {
            Some((refused_start, refused_end)) if refused_start == start => refused_end,
            _ => start,
        };
        for end in first..self.settled {
            let ends_sentence = self
                .current
                .get(end)
                .is_some_and(|word| word.ends_with(['.', '?', '!']));
            if !ends_sentence || end + 1 - start < MIN_SEGMENT_WORDS {
                continue;
            }
            if end + FOLLOWING_WORDS >= self.settled {
                return None;
            }
            let following = self.current.get(end + 1..self.settled)?;
            if !opens_with_cue(following) {
                return Some(end);
            }
        }
        None
    }

    fn context(&self) -> String {
        let outputs: Vec<&str> = self
            .segments
            .iter()
            .filter_map(|segment| match &segment.reply {
                Reply::Done(output) if !output.trim().is_empty() => Some(output.as_str()),
                Reply::Done(_) | Reply::Pending(_) => None,
            })
            .collect();
        context_of(&outputs)
    }
}

/// The last few `outputs`, at most [`CONTEXT_MAX_WORDS`] words of them.
pub(super) fn context_of(outputs: &[&str]) -> String {
    let recent = outputs
        .get(outputs.len().saturating_sub(CONTEXT_SEGMENTS)..)
        .unwrap_or_default()
        .join(" ");
    let words: Vec<&str> = recent.split_whitespace().collect();
    words
        .get(words.len().saturating_sub(CONTEXT_MAX_WORDS)..)
        .unwrap_or_default()
        .join(" ")
}

fn opens_with_cue(words: &[String]) -> bool {
    opens_with(words, HOLD_CUES)
}

/// Whether `words` open with one of `cues`.
fn opens_with(words: &[String], cues: &[&[&str]]) -> bool {
    let opening: Vec<String> = match_words(
        &words
            .iter()
            .take(FOLLOWING_WORDS)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" "),
    );
    cues.iter()
        .any(|cue| cue.len() <= opening.len() && cue.iter().zip(&opening).all(|(c, w)| c == w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(text: &str) -> String {
        text.to_owned()
    }

    /// Feeds `text` as two different drafts (with and without a trailing word), so every word
    /// of `text` has settled.
    fn settle(speculator: &mut Speculator, text: &str) {
        speculator.observe(&format!("{text} uh"));
        speculator.observe(text);
        speculator.observe(&format!("{text} so"));
    }

    const FIRST: &str = "We should move the standup to ten tomorrow morning.";
    const SECOND: &str = "The review can wait until Friday afternoon then.";

    #[test]
    fn a_word_revised_in_the_latest_draft_has_not_settled() {
        let mut speculator = Speculator::new();
        speculator.observe("we should move the");
        speculator.observe("we should move to the");
        assert_eq!(speculator.settled, 3);
        speculator.observe("we should move to the office");
        assert_eq!(speculator.settled, 5);
    }

    #[test]
    fn words_far_enough_from_the_newest_drafts_end_have_settled() {
        let mut speculator = Speculator::new();
        speculator.observe("we should move the standup to ten");
        assert_eq!(speculator.settled, 3);
        speculator.observe("we should move to the stand up to ten tomorrow");
        assert_eq!(speculator.settled, 6);
    }

    #[test]
    fn drafts_come_every_two_seconds_and_at_most_a_fifth_of_the_time() {
        assert!(!draft_due(1_999, 0, 100));
        assert!(draft_due(2_000, 0, 100));
        assert!(!draft_due(12_000, 8_000, 1_000));
        assert!(draft_due(13_000, 8_000, 1_000));
    }

    #[test]
    fn a_segment_waits_for_two_settled_words_after_its_sentence() {
        let mut speculator = Speculator::new();
        settle(&mut speculator, FIRST);
        assert_eq!(speculator.next_request(identity), None);
        settle(&mut speculator, &format!("{FIRST} The review"));
        let request = speculator.next_request(identity);
        assert_eq!(
            request,
            Some(Request {
                id: 0,
                context: String::new(),
                input: FIRST.to_owned(),
            })
        );
    }

    #[test]
    fn short_sentences_merge_until_the_segment_is_long_enough() {
        let mut speculator = Speculator::new();
        settle(
            &mut speculator,
            "Okay. Sounds good to me. Let us ship it today. And more",
        );
        let input = speculator
            .next_request(identity)
            .map(|request| request.input);
        assert_eq!(
            input.as_deref(),
            Some("Okay. Sounds good to me. Let us ship it today.")
        );
    }

    #[test]
    fn a_correction_cue_holds_the_sentence_back() {
        let mut speculator = Speculator::new();
        let text = format!("{FIRST} No wait, make it eleven instead please. And then more");
        settle(&mut speculator, &text);
        let input = speculator
            .next_request(identity)
            .map(|request| request.input);
        assert_eq!(
            input,
            Some(format!("{FIRST} No wait, make it eleven instead please."))
        );
    }

    #[test]
    fn one_request_at_a_time_with_the_formatted_segments_as_context() {
        let mut speculator = Speculator::new();
        settle(&mut speculator, &format!("{FIRST} {SECOND} And more"));
        let Some(first) = speculator.next_request(identity) else {
            panic!("first segment closed");
        };
        assert_eq!(speculator.next_request(identity), None);
        speculator.complete(first.id, Some("FIRST OUT".to_owned()));
        let second = speculator.next_request(identity);
        assert_eq!(
            second.map(|request| (request.context, request.input)),
            Some(("FIRST OUT".to_owned(), SECOND.to_owned()))
        );
    }

    #[test]
    fn a_revised_segment_is_dropped_with_everything_after_it() {
        let mut speculator = Speculator::new();
        settle(&mut speculator, &format!("{FIRST} {SECOND} And more"));
        let Some(first) = speculator.next_request(identity) else {
            panic!("first segment closed");
        };
        speculator.complete(first.id, Some("out".to_owned()));
        assert_eq!(speculator.formatted().len(), 1);
        speculator.observe(&format!("We should move the stand up {SECOND}"));
        assert_eq!(speculator.formatted(), Vec::new());
        assert_eq!(speculator.revised(), 1);
    }

    #[test]
    fn a_refused_segment_goes_again_with_the_next_sentence() {
        let mut speculator = Speculator::new();
        settle(&mut speculator, &format!("{FIRST} {SECOND} And more"));
        let Some(first) = speculator.next_request(identity) else {
            panic!("first segment closed");
        };
        speculator.complete(first.id, None);
        assert_eq!(speculator.formatted(), Vec::new());
        let again = speculator.next_request(identity);
        assert_eq!(
            again.map(|request| (request.context, request.input)),
            Some((String::new(), format!("{FIRST} {SECOND}")))
        );
    }

    #[test]
    fn a_reply_to_a_revised_segment_is_ignored() {
        let mut speculator = Speculator::new();
        settle(&mut speculator, &format!("{FIRST} {SECOND} And more"));
        let Some(first) = speculator.next_request(identity) else {
            panic!("first segment closed");
        };
        speculator.observe("Something else entirely");
        speculator.complete(first.id, Some("late".to_owned()));
        assert_eq!(speculator.formatted(), Vec::new());
    }

    #[test]
    fn context_keeps_the_last_segments_and_words() {
        assert_eq!(context_of(&["a", "b", "c", "d"]), "b c d");
        let long = "word ".repeat(CONTEXT_MAX_WORDS + 5);
        assert_eq!(
            context_of(&[&long]).split_whitespace().count(),
            CONTEXT_MAX_WORDS
        );
    }
}
