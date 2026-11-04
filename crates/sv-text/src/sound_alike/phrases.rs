// FilePath: crates/sv-text/src/sound_alike/phrases.rs
//! Writes a multi-word dictionary entry where the transcript holds the entry's ordinary words
//! and, in place of its name, a word that sounds like it: with "Sienzo Sign on" in the
//! dictionary, "science sign on" becomes "Sienzo Sign on". The entry's own ordinary words are
//! the context that says the speaker meant the name. Anywhere else an ordinary word that sounds
//! like a name ("science") stays as heard, because the speaker may well mean it; the plain-name
//! pass ([`super::correct`]) never touches ordinary words at all.
//!
//! The rules:
//! - The entry has two to [`MAX_WORDS`] words of letters only, at least one of them a name word
//!   (no ordinary English word, four letters or more) and at least one an anchor (an ordinary
//!   English word, or one shorter than four letters) of [`MIN_HEARD_LETTERS`] letters or more.
//! - Every anchor is in the transcript as written, in any case. Two anchors in a row may come
//!   run together ("signon" for "Sign on").
//! - Every name word is in the transcript as written, or as a word of [`MIN_HEARD_LETTERS`]
//!   letters or more whose sounds are at most one edit per three sounds away from it: looser
//!   than the one edit per four of a lone name, because the anchors vouch for it.
//! - The span's words are separated by one space or one hyphen.

use super::{is_ordinary_word, phonetic, Corrected};

/// The most words an entry may have to be written from its context.
const MAX_WORDS: usize = 6;
/// A heard word shorter than this ("a", "is") never stands in for a name, and an entry needs an
/// anchor at least this long: "on" alone says too little about what was meant.
const MIN_HEARD_LETTERS: usize = 3;
/// Shorter words are anchors: the list of ordinary words starts at this length.
const MIN_NAME_LETTERS: usize = 4;
/// A name word is matched by sound within one edit per this many of its sounds.
const SOUNDS_PER_EDIT: usize = 3;

/// A dictionary entry of a name and the ordinary words around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    /// As the user wrote it: what replaces the matched span.
    written: String,
    words: Vec<PhraseWord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PhraseWord {
    folded: String,
    key: Vec<u8>,
    /// No ordinary English word: a heard word that sounds like it is replaced.
    name: bool,
}

impl Phrase {
    /// The entry as a phrase target, or `None` when it is no mix of names and ordinary words.
    pub fn new(phrase: &str) -> Option<Phrase> {
        let written = phrase.trim();
        let mut words = Vec::new();
        for word in written.split(' ') {
            if !word.chars().all(char::is_alphabetic) {
                return None;
            }
            let folded = phonetic::folded(word)?;
            words.push(PhraseWord {
                key: phonetic::key(&folded),
                name: folded.len() >= MIN_NAME_LETTERS && !is_ordinary_word(word),
                folded,
            });
        }
        let has_name = words.iter().any(|word| word.name);
        let has_anchor = words
            .iter()
            .any(|word| !word.name && word.folded.len() >= MIN_HEARD_LETTERS);
        let sized = (2..=MAX_WORDS).contains(&words.len());
        (sized && has_name && has_anchor).then(|| Phrase {
            written: written.to_owned(),
            words,
        })
    }
}

struct Heard {
    start: usize,
    end: usize,
    /// `None` for a word with anything but Latin letters, which matches nothing.
    folded: Option<String>,
}

/// Writes each span that matches a phrase (rules in the module docs) as the phrase.
pub fn correct_phrases(text: &str, phrases: &[Phrase]) -> Corrected {
    let words = heard_words(text);
    let mut out = String::with_capacity(text.len() + 16);
    let mut cursor = 0;
    let mut hits = 0;
    let mut first = 0;
    while first < words.len() {
        let best = phrases
            .iter()
            .filter_map(|phrase| matched(text, &words, first, phrase).map(|end| (end, phrase)))
            .max_by_key(|(end, _)| *end);
        let Some((end, phrase)) = best else {
            first += 1;
            continue;
        };
        let (start, stop) = (words[first].start, words[end - 1].end);
        if text[start..stop] != phrase.written {
            out.push_str(&text[cursor..start]);
            out.push_str(&phrase.written);
            cursor = stop;
            hits += 1;
        }
        first = end;
    }
    out.push_str(&text[cursor..]);
    Corrected { text: out, hits }
}

/// The index after the last transcript word of a match of `phrase` starting at word `first`.
fn matched(text: &str, words: &[Heard], first: usize, phrase: &Phrase) -> Option<usize> {
    let mut at = first;
    let mut index = 0;
    while index < phrase.words.len() {
        let heard = words.get(at)?;
        if at > first && !separated(text, &words[at - 1], heard) {
            return None;
        }
        let folded = heard.folded.as_deref()?;
        let target = &phrase.words[index];
        let used = if folded == target.folded {
            1
        } else if target.name {
            usize::from(sounds_like(folded, target))
        } else {
            match phrase.words.get(index + 1) {
                Some(next) if !next.name && run_together(folded, target, next) => 2,
                _ => 0,
            }
        };
        if used == 0 {
            return None;
        }
        index += used;
        at += 1;
    }
    Some(at)
}

fn sounds_like(folded: &str, target: &PhraseWord) -> bool {
    if folded.len() < MIN_HEARD_LETTERS {
        return false;
    }
    let key = phonetic::key(folded);
    let budget = (key.len().max(target.key.len()) / SOUNDS_PER_EDIT).max(1);
    super::sound_distance(&key, &target.key) <= budget
}

fn run_together(folded: &str, left: &PhraseWord, right: &PhraseWord) -> bool {
    folded.len() == left.folded.len() + right.folded.len()
        && folded.starts_with(&left.folded)
        && folded.ends_with(&right.folded)
}

/// Two words of one span are apart by exactly one space or one hyphen.
fn separated(text: &str, left: &Heard, right: &Heard) -> bool {
    matches!(&text[left.end..right.start], " " | "-")
}

fn heard_words(text: &str) -> Vec<Heard> {
    let mut words = Vec::new();
    let mut start = None;
    for (index, c) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        match (start, c.is_alphabetic()) {
            (None, true) => start = Some(index),
            (Some(from), false) => {
                words.push(Heard {
                    start: from,
                    end: index,
                    folded: phonetic::folded(&text[from..index]),
                });
                start = None;
            }
            _ => {}
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(text: &str, entries: &[&str]) -> String {
        let phrases: Vec<Phrase> = entries.iter().filter_map(|p| Phrase::new(p)).collect();
        correct_phrases(text, &phrases).text
    }

    const ENTRIES: &[&str] = &["Sienzo Sign on", "Kalvi app"];

    #[test]
    fn only_a_mix_of_names_and_ordinary_words_is_a_phrase() {
        assert!(Phrase::new("Sienzo Sign on").is_some());
        for entry in [
            "Sienzo",
            "Penny Trail",
            "rust crates",
            "Ajjwar Hitanshu",
            "ankit/nightly",
            "a b c d e f g",
        ] {
            assert!(Phrase::new(entry).is_none(), "{entry}");
        }
    }

    #[test]
    fn the_words_of_the_entry_let_a_misheard_name_be_written() {
        for heard in [
            "We were working on science sign on today.",
            "We were working on Science Signon today.",
            "We were working on science sign-on today.",
            "We were working on sienzo sign on today.",
        ] {
            assert_eq!(
                fixed(heard, ENTRIES),
                "We were working on Sienzo Sign on today.",
                "{heard}"
            );
        }
        assert_eq!(fixed("Open the culvi app.", ENTRIES), "Open the Kalvi app.");
    }

    #[test]
    fn an_ordinary_word_without_the_context_of_the_entry_stays() {
        for heard in [
            "I love science and computer science.",
            "Science is fun.",
            "The science sign is on the door.",
            "science, sign on",
            "Our science team signs on Monday.",
            "Open the app.",
        ] {
            assert_eq!(fixed(heard, ENTRIES), heard, "{heard}");
        }
    }

    #[test]
    fn a_far_word_is_no_stand_in_for_the_name() {
        assert_eq!(
            fixed("Open the banana app.", ENTRIES),
            "Open the banana app."
        );
        assert_eq!(fixed("the art sign on", ENTRIES), "the art sign on");
    }

    #[test]
    fn hits_count_changed_spans_only() {
        let phrases: Vec<Phrase> = ENTRIES.iter().filter_map(|p| Phrase::new(p)).collect();
        let corrected = correct_phrases("Sienzo Sign on and science sign on", &phrases);
        assert_eq!(corrected.text, "Sienzo Sign on and Sienzo Sign on");
        assert_eq!(corrected.hits, 1);
    }
}
