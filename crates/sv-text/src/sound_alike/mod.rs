// FilePath: crates/sv-text/src/sound_alike/mod.rs
//! Snaps words the speech model misspelled to the personal dictionary's names. It runs inside
//! the deterministic formatter, so it works for every voice model and with formatting off.
//!
//! Parakeet takes no prompt and Whisper's prompt only nudges, so a name the user taught is
//! still heard as "Ajwar", "Adjvar", "Nishanta" or "Angita Tharva". The pass compares how words
//! sound ([`phonetic`]) and only ever writes a dictionary term. The rules, strictest first:
//!
//! - Only dictionary words without a replacement. Names, written with a capital on every word
//!   ("Ajjwar", "ArgoCD", "Penny Trail"), letters only, four sounds or more, are the main
//!   targets. A single lowercase entry that is no ordinary word ("robosienzo", "nuvo") is a
//!   target too and is written in lowercase; lowercase ordinary words ("lint", "pods", "rust")
//!   are words the user wants spelled, never targets. An entry that is neither ("ankit/nightly",
//!   "Ankit fmehra") gives its parts instead: each part between spaces or slashes that is a
//!   name, and for a slash-joined entry also each lowercase part that is no ordinary word.
//! - Names snap only words the model capitalised, so it took them for a name; a lowercase
//!   target snaps one word of any case.
//! - Never an ordinary English word ([`common_words.txt`](common_words.txt)): "Cargo" stays
//!   "Cargo" with "Kargo" in the dictionary, at a sentence start or anywhere else.
//! - Never a word that already is a dictionary entry, in any case.
//! - The sounds must be close: at most one edit per four sounds (similarity 0.75). "Nishanta" and
//!   "Nishan" become "Nishant" because "Nishant" is in the dictionary and one sound apart; with
//!   no such entry they stay as heard. A word of four sounds or fewer keeps its number of
//!   sounds: "Kulvi" becomes "Kalvi", but the real name "Kali" never does. A target of five
//!   sounds or more also takes a word that differs from it only in vowels and breath `h`s
//!   ("Angkit", "Unket" → "Ankit"; "Itansho" → "Hitanshu"), since accents move vowels far more
//!   than consonants.
//! - A run of up to three capitalised words can become up to three dictionary words, which
//!   repairs merged words ("Ankitatharva" → "Ankit Atharva") and split ones ("Angita Tharva" →
//!   "Ankit Atharva", "Ajwari Tanshu" → "Ajjwar Hitanshu").

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
    /// Written in lowercase ("robosienzo"): snaps one word of any case, not only a name.
    lowercase_target: bool,
}

/// A target of this many sounds also takes a word only its vowels apart
/// ([`phonetic::vowels_apart`]); shorter names would match too much by chance.
const VOWEL_TOLERANT_SOUNDS: usize = 5;

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

/// An ordinary English word in any case ("Snowflake", "iceberg"), which no rule may recase.
pub(crate) fn is_ordinary_word(word: &str) -> bool {
    phonetic::folded(word).is_some_and(|folded| is_common(&folded))
}

/// The sound key of one word, or `None` when it holds anything but Latin letters.
pub(crate) fn sound_key(word: &str) -> Option<Vec<u8>> {
    phonetic::folded(word).map(|folded| phonetic::key(&folded))
}

/// The edit distance between two sound keys.
pub(crate) fn sound_distance(a: &[u8], b: &[u8]) -> usize {
    phonetic::prefix_distances(a, b)[b.len()]
}

/// Sound keys of the ordinary words, built on first use (a few milliseconds, once).
static COMMON_SOUNDS: LazyLock<std::collections::HashSet<Vec<u8>>> =
    LazyLock::new(|| COMMON.iter().map(|word| phonetic::key(word)).collect());

impl Spelling {
    /// The entry as a sound-alike target: a name, or one lowercase word that is no ordinary
    /// word. `None` for anything else.
    pub(crate) fn new(phrase: &str) -> Option<Spelling> {
        let words: Vec<&str> = phrase.split(' ').collect();
        if words.len() > MAX_SPAN {
            return None;
        }
        let starts_upper = |word: &str| word.chars().next().is_some_and(char::is_uppercase);
        let lowercase_target = words.len() == 1 && !starts_upper(words[0]);
        let mut folded = String::new();
        let mut key = Vec::new();
        for word in &words {
            if !lowercase_target && !starts_upper(word) {
                return None;
            }
            let word_folded = phonetic::folded(word)?;
            key.extend(phonetic::key(&word_folded));
            folded.push_str(&word_folded);
        }
        if lowercase_target && is_common(&folded) {
            return None;
        }
        (key.len() >= MIN_SOUNDS).then(|| Spelling {
            written: phrase.to_owned(),
            folded,
            lowercase: phrase.to_lowercase(),
            key,
            words: words.len(),
            lowercase_target,
        })
    }

    /// The targets a dictionary entry gives (rules in the module docs): the entry itself, or
    /// else its parts.
    pub(crate) fn from_entry(phrase: &str) -> Vec<Spelling> {
        if let Some(whole) = Spelling::new(phrase) {
            return vec![whole];
        }
        let slashed = phrase.contains('/');
        phrase
            .split([' ', '/'])
            .filter(|part| part.chars().next().is_some_and(char::is_uppercase) || slashed)
            .filter(|part| !is_ordinary_word(part))
            .filter_map(Spelling::new)
            .collect()
    }

    /// One word, as the Parakeet booster takes its terms.
    pub fn is_single_word(&self) -> bool {
        self.words == 1
    }

    /// An ordinary English word, or a name that sounds exactly like one ("Kargo" and "cargo"):
    /// Parakeet's booster matches by sound, so it would write such a name over the word.
    pub fn sounds_like_ordinary_word(&self) -> bool {
        self.words == 1 && (is_common(&self.folded) || COMMON_SOUNDS.contains(&self.key))
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
    /// May join a span of names: capitalised, Latin letters, not an ordinary or already
    /// correct word.
    name: bool,
    /// May become a lowercase target: the same in any case.
    loose: bool,
}

struct Candidate {
    len: usize,
    /// 0..=1000: one minus the distance over the longer side's sounds.
    similarity: usize,
    replacement: String,
}

impl Candidate {
    /// Covering more words always wins, so a misheard name split over two words ("Angora
    /// Tharva") is repaired as a whole instead of only its closer half. At equal coverage the
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

/// Every span of capitalised words that sounds close to a run of names, and every single word
/// close to a lowercase target, grouped by the span's first word. Spans that would come out
/// unchanged are left out.
fn candidates(text: &str, words: &[Word], spellings: &[Spelling]) -> Vec<Vec<Candidate>> {
    let mut by_first: Vec<Vec<Candidate>> = Vec::with_capacity(words.len());
    for first in 0..words.len() {
        let mut here = Vec::new();
        let mut key = Vec::new();
        for len in 1..=MAX_SPAN.min(words.len() - first) {
            let last = first + len - 1;
            if !words[last].name || (len > 1 && !joined(text, &words[last - 1], &words[last])) {
                break;
            }
            key.extend_from_slice(&words[last].key);
            here.extend(candidate(
                text,
                &words[first..=last],
                &key,
                spellings,
                false,
            ));
        }
        // A capitalised word goes to a lowercase target only when no name took it.
        if here.is_empty() && words[first].loose {
            let word = &words[first..=first];
            here.extend(candidate(text, word, &word[0].key, spellings, true));
        }
        by_first.push(here);
    }
    by_first
}

fn candidate(
    text: &str,
    span: &[Word],
    key: &[u8],
    spellings: &[Spelling],
    lowercase: bool,
) -> Option<Candidate> {
    let (first, last) = (span.first()?, span.last()?);
    let (distance, terms) = best_match(key, spellings, lowercase)?;
    let written: Vec<&str> = terms
        .iter()
        .map(|&t| spellings[t].written.as_str())
        .collect();
    let replacement = written.join(" ");
    if text[first.start..last.end] == replacement {
        return None;
    }
    let sounds: usize = terms.iter().map(|&t| spellings[t].key.len()).sum();
    Some(Candidate {
        len: span.len(),
        similarity: 1000 - distance * 1000 / key.len().max(sounds).max(1),
        replacement,
    })
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
    let Some(folded) = phonetic::folded(written) else {
        return Word {
            start,
            end,
            key: Vec::new(),
            name: false,
            loose: false,
        };
    };
    let capitalised = written.chars().next().is_some_and(char::is_uppercase);
    let lowercase = written.to_lowercase();
    let loose = folded.len() >= MIN_WORD_LETTERS
        && !is_common(&folded)
        && !spellings.iter().any(|s| s.lowercase == lowercase);
    Word {
        start,
        end,
        key: phonetic::key(&folded),
        name: capitalised && loose,
        loose,
    }
}

/// The closest run of up to [`MAX_SPAN`] names (or one lowercase target) to `key`, as
/// (distance, spelling indexes), when it is close enough to count: one edit per four sounds of
/// the longer side, or only vowels apart from one target of [`VOWEL_TOLERANT_SOUNDS`] or more.
/// A heard word shorter than that only takes a target of its own length: a real short name
/// ("Kali", "Ravi") is one added sound away from many dictionary names ("Kalvi", "Ravin"),
/// while a misheard name keeps its number of sounds and swaps one.
fn best_match(key: &[u8], spellings: &[Spelling], lowercase: bool) -> Option<(usize, Vec<usize>)> {
    let terms = if lowercase { 1 } else { MAX_SPAN };
    if let Some(found) = search(key, spellings, lowercase, terms, key.len() / 4 + 1) {
        let written: usize = found.1.iter().map(|&t| spellings[t].key.len()).sum();
        let grows_short_word = key.len() < VOWEL_TOLERANT_SOUNDS && written != key.len();
        if found.0 * 4 <= key.len().max(written) && !grows_short_word {
            return Some(found);
        }
    }
    spellings
        .iter()
        .enumerate()
        .filter(|(_, s)| s.lowercase_target == lowercase && s.key.len() >= VOWEL_TOLERANT_SOUNDS)
        .filter(|(_, s)| phonetic::vowels_apart(key, &s.key))
        .map(|(index, s)| (sound_distance(&s.key, key), vec![index]))
        .min_by_key(|(distance, _)| *distance)
}

/// The closest run of at most `terms_left` spellings to `key` within `budget` edits. Each
/// spelling must also be close on its own (about one edit per three of its sounds), which keeps
/// the search to the few spellings that plausibly start the span.
fn search(
    key: &[u8],
    spellings: &[Spelling],
    lowercase: bool,
    terms_left: usize,
    budget: usize,
) -> Option<(usize, Vec<usize>)> {
    let mut best: Option<(usize, Vec<usize>)> = None;
    let better = |best: &Option<(usize, Vec<usize>)>, distance: usize, terms: usize| {
        best.as_ref()
            .is_none_or(|(d, t)| distance < *d || (distance == *d && terms < t.len()))
    };
    for (index, spelling) in spellings.iter().enumerate() {
        if spelling.lowercase_target != lowercase {
            continue;
        }
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
            let Some((tail, mut rest)) = search(
                &key[split..],
                spellings,
                lowercase,
                terms_left - 1,
                budget - head,
            ) else {
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

    const TEAM: &[&str] = &["Ajjwar", "Hitanshu", "Nishant", "Swati", "Ankit", "Atharva"];

    fn fixed(text: &str, phrases: &[&str]) -> String {
        correct(text, &spellings(phrases)).text
    }

    #[test]
    fn names_only_become_targets_when_written_as_names() {
        assert!(Spelling::new("Ajjwar").is_some());
        assert!(Spelling::new("Penny Trail").is_some());
        assert!(Spelling::new("ArgoCD").is_some());
        for phrase in [
            "lint",
            "pods",
            "rust",
            "Penny trail",
            "Amp",
            "AGENTS.md",
            "ankit/nightly",
        ] {
            assert!(Spelling::new(phrase).is_none(), "{phrase}");
        }
        assert!(Spelling::new("Just recreate web").is_none());
        assert!(Spelling::new("A B C D").is_none());
    }

    #[test]
    fn the_captains_three_dictations_come_out_right() {
        let expected = "Ajjwar Hitanshu Nishant Swati Ankit Atharva.";
        for heard in [
            "Ajwar Hitanšu Nishan Swati Ankitatharva.",
            "Ajwar Hitanju Nishanta Swati Angita Tharva.",
            "Ajwari Tanshu Nishanta Swati Ankita Tharva.",
        ] {
            assert_eq!(fixed(heard, TEAM), expected, "{heard}");
        }
    }

    #[test]
    fn parakeet_misspellings_of_the_synthetic_clips() {
        assert_eq!(
            fixed("Please ask Adjavar and Hitanshuk to review it.", TEAM),
            "Please ask Ajjwar and Hitanshu to review it."
        );
        assert_eq!(
            fixed("Nishunt will call Ankit and Atherva.", TEAM),
            "Nishant will call Ankit and Atharva."
        );
        assert_eq!(
            fixed("Ajjavarhitanshu Nishant Swatirankita Tharva.", TEAM),
            "Ajjwar Hitanshu Nishant Swati Ankit Atharva."
        );
        assert_eq!(
            fixed("Kulvi should open it.", &["Kalvi"]),
            "Kalvi should open it."
        );
    }

    #[test]
    fn a_long_name_takes_a_word_only_its_vowels_apart() {
        for heard in ["Angkit", "Unket"] {
            assert_eq!(
                fixed(&format!("{heard} pushed the fix."), TEAM),
                "Ankit pushed the fix.",
                "{heard}"
            );
        }
        assert_eq!(fixed("Itansho is here.", TEAM), "Hitanshu is here.");
    }

    #[test]
    fn a_short_real_name_never_grows_a_sound_to_become_a_dictionary_name() {
        assert_eq!(
            fixed("Kali is a goddess.", &["Kalvi"]),
            "Kali is a goddess."
        );
        for heard in ["Kulvi", "Khalvi", "Culvy"] {
            assert_eq!(
                fixed(&format!("{heard} should open it."), &["Kalvi"]),
                "Kalvi should open it.",
                "{heard}"
            );
        }
        assert_eq!(
            fixed("Meet Nishan today.", &["Nishant"]),
            "Meet Nishant today."
        );
    }

    #[test]
    fn counts_each_corrected_place_once() {
        let out = correct("Ajwar Hitanšu Nishan Swati Ankitatharva.", &spellings(TEAM));
        assert_eq!(out.hits, 4);
        assert_eq!(correct("Swati and Ankit.", &spellings(TEAM)).hits, 0);
    }

    #[test]
    fn ordinary_words_are_never_rewritten() {
        let dictionary = &[
            "Kargo", "Groq", "Nishant", "Sienzo", "Kalvi", "Polaris", "Harbor", "Dhaval", "Hurl",
            "Curl", "Kitty", "Agents", "Crates", "Rupak", "Ankit",
        ];
        for text in [
            "Cargo builds the crate.",
            "Use cargo and grok it.",
            "Science of progress, science everywhere.",
            "Grow the team. Drag it here. Lent me a pen.",
            "the nishanta school of thought",
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
            fixed("Meet Nishanta today.", &["Nishant"]),
            "Meet Nishant today."
        );
        assert_eq!(
            fixed("Meet Nishanta today.", &["Kalvi"]),
            "Meet Nishanta today."
        );
        // Two edits on a six-sound name is too far.
        assert_eq!(fixed("Ask Rajesh.", &["Rajveep"]), "Ask Rajesh.");
        assert_eq!(fixed("Ask Rupika.", &["Rupak"]), "Ask Rupika.");
    }

    #[test]
    fn keeps_punctuation_and_surrounding_text() {
        assert_eq!(
            fixed(
                "Hi Ajwar, meet Nishan (and Kulvi)!",
                &["Ajjwar", "Nishant", "Kalvi"]
            ),
            "Hi Ajjwar, meet Nishant (and Kalvi)!"
        );
        assert_eq!(fixed("Ajwar's laptop", TEAM), "Ajjwar's laptop");
    }

    #[test]
    fn words_across_punctuation_or_lines_never_merge() {
        assert_eq!(fixed("Angita. Tharva", TEAM), "Angita. Atharva");
        // A run-on name is still repaired on its own; it just never joins the next line.
        assert_eq!(fixed("Ajwari\nTanshu", TEAM), "Ajjwar\nTanshu");
    }

    #[test]
    fn non_latin_text_passes_through() {
        assert_eq!(fixed("Кибда Адар.", TEAM), "Кибда Адар.");
        assert_eq!(fixed("कल मीटिंग है", TEAM), "कल मीटिंग है");
    }

    #[test]
    fn multi_word_names_are_targets_too() {
        assert_eq!(
            fixed("Ask Swatirao now.", &["Swati Rao"]),
            "Ask Swati Rao now."
        );
        // An ordinary word never joins a span, even inside a multi-word name.
        assert_eq!(
            fixed("Open Peni Trail now.", &["Penny Trail"]),
            "Open Peni Trail now."
        );
    }

    #[test]
    fn the_common_word_list_is_sorted_for_binary_search() {
        assert!(COMMON.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(is_common("cargo") && is_common("science") && !is_common("nishanta"));
    }

    #[test]
    fn names_that_sound_like_ordinary_words_are_flagged() {
        let flagged =
            |phrase: &str| Spelling::new(phrase).is_some_and(|s| s.sounds_like_ordinary_word());
        assert!(flagged("Kargo") && flagged("Agents") && flagged("Kitty"));
        for name in [
            "Groq", "Ajjwar", "Hitanshu", "Nishant", "Swati", "Ankit", "Atharva", "Kalvi",
        ] {
            assert!(!flagged(name), "{name}");
        }
    }
}
