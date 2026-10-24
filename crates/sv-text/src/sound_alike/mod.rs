// FilePath: crates/sv-text/src/sound_alike/mod.rs
//! Snaps words the speech model misspelled to the personal dictionary's names. It runs inside
//! the deterministic formatter, so it works for every voice model and with formatting off.
//!
//! Parakeet takes no prompt and Whisper's prompt only nudges, so a name the user taught is
//! still heard as "Ujwal", "Udjval", "Vedanta" or "Angora Gastya". The pass compares how words
//! sound ([`phonetic`]) and only ever writes a dictionary term. The rules, strictest first:
//!
//! - Only dictionary words without a replacement, written as names: every word starts with a
//!   capital ("Ujjwal", "ArgoCD", "Money Trace"), letters only, four sounds or more. Lowercase
//!   entries ("lint", "pods", "rust") are ordinary words the user wants spelled, never targets.
//! - Only words the model capitalised, so it took them for a name. A lowercase word is never
//!   rewritten.
//! - Never an ordinary English word ([`common_words.txt`](common_words.txt)): "Cargo" stays
//!   "Cargo" with "Kargo" in the dictionary, at a sentence start or anywhere else.
//! - Never a word that already is a dictionary entry, in any case.
//! - The sounds must be close: at most one edit per four sounds (similarity 0.75). "Vedanta" and
//!   "Vedan" become "Vedant" because "Vedant" is in the dictionary and one sound apart; with
//!   no such entry they stay as heard.
//! - A run of up to three capitalised words can become up to three dictionary words, which
//!   repairs merged words ("Ankuragastya" → "Ankur Agastya") and split ones ("Angora Gastya" →
//!   "Ankur Agastya", "Ujwali Manshu" → "Ujjwal Himanshu").

mod phonetic;

use std::sync::LazyLock;

/// A dictionary entry that close-sounding transcript words snap to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spelling {
    /// As the user wrote it; what replaces the misheard words.
    pub written: String,
    /// Lowercase ASCII letters of the whole entry, words run together.
    folded: String,
    /// The entry lowercased as written, to tell a word that already is this entry.
    lowercase: String,
    /// Sound keys of the entry's words, run together.
    key: Vec<u8>,
    words: usize,
}

/// Fewer sounds than this match too much by chance ("Amp", "Cmd").
const MIN_SOUNDS: usize = 4;
/// The most transcript words one match covers, and the most dictionary words it writes.
const MAX_SPAN: usize = 3;
/// A transcript word shorter than this ("A", "In") never joins a span.
const MIN_WORD_LETTERS: usize = 3;

static COMMON: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    include_str!("common_words.txt")
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect()
});

fn is_common(folded: &str) -> bool {
    COMMON.binary_search(&folded).is_ok()
}

impl Spelling {
    /// The entry as a sound-alike target, or `None` when it is not written as a name.
    pub(crate) fn new(phrase: &str) -> Option<Spelling> {
        let words: Vec<&str> = phrase.split(' ').collect();
        if words.len() > MAX_SPAN {
            return None;
        }
        let mut folded = String::new();
        let mut key = Vec::new();
        for word in &words {
            if !word.chars().next().is_some_and(char::is_uppercase) {
                return None;
            }
            let word_folded = phonetic::folded(word)?;
            key.extend(phonetic::key(&word_folded));
            folded.push_str(&word_folded);
        }
        (key.len() >= MIN_SOUNDS).then(|| Spelling {
            written: phrase.to_owned(),
            folded,
            lowercase: phrase.to_lowercase(),
            key,
            words: words.len(),
        })
    }

    /// One word, as the Parakeet booster takes its terms.
    pub fn is_single_word(&self) -> bool {
        self.words == 1
    }

    /// An ordinary English word the user capitalised ("Agents", "Graphics").
    pub fn is_ordinary_word(&self) -> bool {
        self.words == 1 && is_common(&self.folded)
    }
}

/// The text with misheard names written as dictionary terms, and how many places changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Corrected {
    pub text: String,
    pub hits: u32,
}

struct Word {
    start: usize,
    end: usize,
    key: Vec<u8>,
    /// May join a span: capitalised, Latin letters, not an ordinary or already correct word.
    eligible: bool,
}

struct Candidate {
    len: usize,
    /// 0..=1000: one minus the distance over the longer side's sounds.
    similarity: usize,
    replacement: String,
}

impl Candidate {
    /// Covering more words always wins, so a misheard name split over two words ("Angora
    /// Gastya") is repaired as a whole instead of only its closer half. At equal coverage the
    /// similarities add up, so two separately close names beat one run-together match.
    fn score(&self) -> usize {
        self.len * 2000 + self.similarity
    }
}

/// Snaps capitalised words that sound like a dictionary name to it (rules in the module docs).
pub fn correct(text: &str, spellings: &[Spelling]) -> Corrected {
    if spellings.is_empty() {
        return Corrected {
            text: text.to_owned(),
            hits: 0,
        };
    }
    let words = words(text, spellings);
    let candidates = candidates(text, &words, spellings);

    // best[i]: the highest total score over words[i..], and the candidate starting at i it takes.
    let mut best: Vec<(usize, Option<usize>)> = vec![(0, None); words.len() + 1];
    for first in (0..words.len()).rev() {
        best[first] = (best[first + 1].0, None);
        for (index, candidate) in candidates[first].iter().enumerate() {
            let total = candidate.score() + best[first + candidate.len].0;
            if total > best[first].0 {
                best[first] = (total, Some(index));
            }
        }
    }

    let mut out = String::with_capacity(text.len() + 16);
    let mut cursor = 0;
    let mut hits = 0;
    let mut first = 0;
    while first < words.len() {
        let Some(index) = best[first].1 else {
            first += 1;
            continue;
        };
        let candidate = &candidates[first][index];
        out.push_str(&text[cursor..words[first].start]);
        out.push_str(&candidate.replacement);
        cursor = words[first + candidate.len - 1].end;
        hits += 1;
        first += candidate.len;
    }
    out.push_str(&text[cursor..]);
    Corrected { text: out, hits }
}

/// Every span of eligible words that sounds close to a run of spellings, grouped by the span's
/// first word. Spans that would come out unchanged are left out.
fn candidates(text: &str, words: &[Word], spellings: &[Spelling]) -> Vec<Vec<Candidate>> {
    let mut by_first: Vec<Vec<Candidate>> = Vec::with_capacity(words.len());
    for first in 0..words.len() {
        let mut here = Vec::new();
        let mut key = Vec::new();
        for len in 1..=MAX_SPAN.min(words.len() - first) {
            let last = first + len - 1;
            if !words[last].eligible || (len > 1 && !joined(text, &words[last - 1], &words[last])) {
                break;
            }
            key.extend_from_slice(&words[last].key);
            let Some((distance, terms)) = best_match(&key, spellings) else {
                continue;
            };
            let written: Vec<&str> = terms
                .iter()
                .map(|&t| spellings[t].written.as_str())
                .collect();
            let replacement = written.join(" ");
            if text[words[first].start..words[last].end] == replacement {
                continue;
            }
            let sounds: usize = terms.iter().map(|&t| spellings[t].key.len()).sum();
            here.push(Candidate {
                len,
                similarity: 1000 - distance * 1000 / key.len().max(sounds).max(1),
                replacement,
            });
        }
        by_first.push(here);
    }
    by_first
}

/// Two words of one span are separated by spaces only.
fn joined(text: &str, left: &Word, right: &Word) -> bool {
    let gap = &text[left.end..right.start];
    !gap.is_empty() && gap.bytes().all(|b| b == b' ')
}

fn words(text: &str, spellings: &[Spelling]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (start, c.is_alphabetic()) {
            (None, true) => start = Some(index),
            (Some(from), false) => {
                words.push(word(&text[from..index], from, spellings));
                start = None;
            }
            _ => {}
        }
    }
    words
}

fn word(written: &str, start: usize, spellings: &[Spelling]) -> Word {
    let end = start + written.len();
    let capitalised = written.chars().next().is_some_and(char::is_uppercase);
    let folded = capitalised.then(|| phonetic::folded(written)).flatten();
    let Some(folded) = folded else {
        return Word {
            start,
            end,
            key: Vec::new(),
            eligible: false,
        };
    };
    let lowercase = written.to_lowercase();
    let eligible = folded.len() >= MIN_WORD_LETTERS
        && !is_common(&folded)
        && !spellings.iter().any(|s| s.lowercase == lowercase);
    Word {
        start,
        end,
        key: phonetic::key(&folded),
        eligible,
    }
}

/// The closest run of up to [`MAX_SPAN`] spellings to `key`, as (distance, spelling indexes),
/// when it is close enough to count: one edit per four sounds of the longer side.
fn best_match(key: &[u8], spellings: &[Spelling]) -> Option<(usize, Vec<usize>)> {
    let found = search(key, spellings, MAX_SPAN, key.len() / 4 + 1)?;
    let written: usize = found.1.iter().map(|&t| spellings[t].key.len()).sum();
    (found.0 * 4 <= key.len().max(written)).then_some(found)
}

/// The closest run of at most `terms_left` spellings to `key` within `budget` edits. Each
/// spelling must also be close on its own (about one edit per three of its sounds), which keeps
/// the search to the few spellings that plausibly start the span.
fn search(
    key: &[u8],
    spellings: &[Spelling],
    terms_left: usize,
    budget: usize,
) -> Option<(usize, Vec<usize>)> {
    let mut best: Option<(usize, Vec<usize>)> = None;
    let better = |best: &Option<(usize, Vec<usize>)>, distance: usize, terms: usize| {
        best.as_ref()
            .is_none_or(|(d, t)| distance < *d || (distance == *d && terms < t.len()))
    };
    for (index, spelling) in spellings.iter().enumerate() {
        let local = budget.min((spelling.key.len() + 1) / 3);
        let prefixes = phonetic::prefix_distances(&spelling.key, key);
        let whole = prefixes[key.len()];
        if whole <= local && better(&best, whole, 1) {
            best = Some((whole, vec![index]));
        }
        if terms_left < 2 {
            continue;
        }
        // This spelling covers the start of the span and the others the rest.
        for split in 2..key.len().saturating_sub(1) {
            let head = prefixes[split];
            if head > local {
                continue;
            }
            let Some((tail, mut rest)) =
                search(&key[split..], spellings, terms_left - 1, budget - head)
            else {
                continue;
            };
            if better(&best, head + tail, rest.len() + 1) {
                rest.insert(0, index);
                best = Some((head + tail, rest));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spellings(phrases: &[&str]) -> Vec<Spelling> {
        phrases.iter().filter_map(|p| Spelling::new(p)).collect()
    }

    const TEAM: &[&str] = &["Ujjwal", "Himanshu", "Vedant", "Shilpa", "Ankur", "Agastya"];

    fn fixed(text: &str, phrases: &[&str]) -> String {
        correct(text, &spellings(phrases)).text
    }

    #[test]
    fn names_only_become_targets_when_written_as_names() {
        assert!(Spelling::new("Ujjwal").is_some());
        assert!(Spelling::new("Money Trace").is_some());
        assert!(Spelling::new("ArgoCD").is_some());
        for phrase in [
            "lint",
            "pods",
            "rust",
            "Money trace",
            "Amp",
            "AGENTS.md",
            "ankur/nightly",
        ] {
            assert!(Spelling::new(phrase).is_none(), "{phrase}");
        }
        assert!(Spelling::new("Just recreate web").is_none());
        assert!(Spelling::new("A B C D").is_none());
    }

    #[test]
    fn the_captains_three_dictations_come_out_right() {
        let expected = "Ujjwal Himanshu Vedant Shilpa Ankur Agastya.";
        for heard in [
            "Ujwal Himanšu Vedan Shilpa Ankuragastya.",
            "Ujwal Himanju Vedanta Shilpa Angora Gastya.",
            "Ujwali Manshu Vedanta Shilpa Ankura Gastya.",
        ] {
            assert_eq!(fixed(heard, TEAM), expected, "{heard}");
        }
    }

    #[test]
    fn parakeet_misspellings_of_the_synthetic_clips() {
        assert_eq!(
            fixed("Please ask Udjaval and Himanshuk to review it.", TEAM),
            "Please ask Ujjwal and Himanshu to review it."
        );
        assert_eq!(
            fixed("Vidant will call Ankur and Agustya.", TEAM),
            "Vedant will call Ankur and Agastya."
        );
        assert_eq!(
            fixed("Ujjavalhimanshu Vedant Shilparankura Gastya.", TEAM),
            "Ujjwal Himanshu Vedant Shilpa Ankur Agastya."
        );
        assert_eq!(
            fixed("Kulki should open it.", &["Kalki"]),
            "Kalki should open it."
        );
    }

    #[test]
    fn counts_each_corrected_place_once() {
        let out = correct("Ujwal Himanšu Vedan Shilpa Ankuragastya.", &spellings(TEAM));
        assert_eq!(out.hits, 4);
        assert_eq!(correct("Shilpa and Ankur.", &spellings(TEAM)).hits, 0);
    }

    #[test]
    fn ordinary_words_are_never_rewritten() {
        let dictionary = &[
            "Kargo", "Groq", "Vedant", "Signzy", "Kalki", "Polaris", "Harbor", "Dhruv", "Hurl",
            "Curl", "Kitty", "Agents", "Crates", "Deepak", "Ankur",
        ];
        for text in [
            "Cargo builds the crate.",
            "Use cargo and grok it.",
            "Signs of progress, signs everywhere.",
            "Grow the team. Drag it here. Lent me a pen.",
            "the vedanta school of thought",
            "Deep work and anchor points.",
            "Curly braces and hurled stones.",
            "I think it is fine.",
        ] {
            assert_eq!(fixed(text, dictionary), text, "{text}");
        }
    }

    #[test]
    fn a_close_name_wins_only_when_it_is_in_the_dictionary() {
        assert_eq!(
            fixed("Meet Vedanta today.", &["Vedant"]),
            "Meet Vedant today."
        );
        assert_eq!(
            fixed("Meet Vedanta today.", &["Kalki"]),
            "Meet Vedanta today."
        );
        // Two edits on a six-sound name is too far.
        assert_eq!(fixed("Ask Rajesh.", &["Rajdeep"]), "Ask Rajesh.");
        assert_eq!(fixed("Ask Deepika.", &["Deepak"]), "Ask Deepika.");
    }

    #[test]
    fn keeps_punctuation_and_surrounding_text() {
        assert_eq!(
            fixed(
                "Hi Ujwal, meet Vedan (and Kulki)!",
                &["Ujjwal", "Vedant", "Kalki"]
            ),
            "Hi Ujjwal, meet Vedant (and Kalki)!"
        );
        assert_eq!(fixed("Ujwal's laptop", TEAM), "Ujjwal's laptop");
    }

    #[test]
    fn words_across_punctuation_or_lines_never_merge() {
        assert_eq!(fixed("Angora. Gastya", TEAM), "Angora. Agastya");
        // A run-on name is still repaired on its own; it just never joins the next line.
        assert_eq!(fixed("Ujwali\nManshu", TEAM), "Ujjwal\nManshu");
    }

    #[test]
    fn non_latin_text_passes_through() {
        assert_eq!(fixed("Кибда Адар.", TEAM), "Кибда Адар.");
        assert_eq!(fixed("कल मीटिंग है", TEAM), "कल मीटिंग है");
    }

    #[test]
    fn multi_word_names_are_targets_too() {
        assert_eq!(
            fixed("Ask Shilparao now.", &["Shilpa Rao"]),
            "Ask Shilpa Rao now."
        );
        // An ordinary word never joins a span, even inside a multi-word name.
        assert_eq!(
            fixed("Open Mani Trace now.", &["Money Trace"]),
            "Open Mani Trace now."
        );
    }

    #[test]
    fn the_common_word_list_is_sorted_for_binary_search() {
        assert!(COMMON.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(is_common("cargo") && is_common("signs") && !is_common("vedanta"));
    }
}
