// FilePath: crates/sv-text/src/packs/mod.rs
//! Built-in vocabulary: the product's own name, always on, and the packs a user picks on the
//! Dictionary page. They join the personal dictionary in [`crate::Vocabulary`], behind it.
//!
//! Curation rules for the lists, because a bad entry misfires in every dictation:
//! - Words are ordered by what recognition gains from them: odd casing, brand spellings and
//!   terms Whisper splits apart come before words it already writes well. Only the head of a
//!   list reaches the recognition prompt, so the order is the priority.
//! - A single word that starts with a letter and has a capital after it ("GitHub", "SaaS",
//!   "EC2") also becomes a casing rule, applied case-insensitively to whole words. Never list
//!   one whose lowercase form is an ordinary English or romanised Hindi word ("IT", "RAM",
//!   "SPA", "MoM", "LoRA", "SAP", "PE"): every such word in a dictation would be capitalised.
//! - A rule rewrites how a term is heard ("node js") into how it is written ("Node.js"). Never
//!   add one whose heard side is a normal phrase ("linked in", "sales force", "open ai").

mod backend;
mod business;
mod cloud;
mod data_ai;
mod everyday;
mod marketing;
mod mobile;
mod programming;
mod sales;
mod web;

use sv_domain::{VocabularyPack, VocabularyPackInfo};

pub(crate) struct Pack {
    /// Spellings, most useful first.
    pub(crate) words: &'static [&'static str],
    /// `(heard, written)` pairs.
    pub(crate) rules: &'static [(&'static str, &'static str)],
}

/// The product's name, ahead of every other prompt word. Only the joined spelling becomes a
/// casing rule: "simple voice" is also an ordinary phrase.
pub(crate) const PRODUCT: Pack = Pack {
    words: &["Simple Voice", "SimpleVoice"],
    rules: &[],
};

pub(crate) fn pack(id: VocabularyPack) -> &'static Pack {
    match id {
        VocabularyPack::Everyday => &everyday::PACK,
        VocabularyPack::Programming => &programming::PACK,
        VocabularyPack::Web => &web::PACK,
        VocabularyPack::Backend => &backend::PACK,
        VocabularyPack::Mobile => &mobile::PACK,
        VocabularyPack::Cloud => &cloud::PACK,
        VocabularyPack::DataAi => &data_ai::PACK,
        VocabularyPack::Sales => &sales::PACK,
        VocabularyPack::Marketing => &marketing::PACK,
        VocabularyPack::Business => &business::PACK,
    }
}

/// Whether a built-in word is also a casing rule (see the module comment).
pub(crate) fn is_casing_word(word: &str) -> bool {
    let mut chars = word.chars();
    let starts_with_letter = chars.next().is_some_and(char::is_alphabetic);
    starts_with_letter && !word.contains(char::is_whitespace) && chars.any(char::is_uppercase)
}

/// Every pack with the spellings it teaches: its words, then the written side of each rule
/// not already among them.
pub fn vocabulary_pack_infos() -> Vec<VocabularyPackInfo> {
    VocabularyPack::ALL
        .into_iter()
        .map(|id| {
            let pack = pack(id);
            let mut terms: Vec<String> = pack.words.iter().map(|word| (*word).to_owned()).collect();
            for (_, written) in pack.rules {
                if !terms.iter().any(|term| term == written) {
                    terms.push((*written).to_owned());
                }
            }
            VocabularyPackInfo {
                id,
                name: id.name().to_owned(),
                description: id.description().to_owned(),
                terms,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Lowercase forms that are ordinary words in English or romanised Hindi, or units: a casing
    /// rule for any of them would rewrite normal speech.
    const ORDINARY: &[&str] = &[
        "ai", "am", "as", "acid", "cogs", "db", "do", "dom", "go", "grr", "ho", "hr", "id", "in",
        "is", "it", "jax", "lora", "ml", "mom", "na", "no", "on", "ooo", "or", "pe", "rag", "ram",
        "rest", "sam", "sap", "se", "so", "sow", "spa", "tam", "to", "us",
    ];

    fn every_pack() -> impl Iterator<Item = (&'static str, &'static Pack)> {
        VocabularyPack::ALL
            .into_iter()
            .map(|id| (id.name(), pack(id)))
            .chain([("product", &PRODUCT)])
    }

    #[test]
    fn words_are_trimmed_and_listed_once_per_pack() {
        for (name, pack) in every_pack() {
            let mut seen = HashSet::new();
            for word in pack.words {
                assert!(!word.is_empty() && word.trim() == *word, "{name}: {word:?}");
                assert!(seen.insert(word.to_lowercase()), "{name}: {word} twice");
            }
        }
    }

    #[test]
    fn rules_are_heard_in_lowercase_once_and_change_something() {
        for (name, pack) in every_pack() {
            let mut seen = HashSet::new();
            for (heard, written) in pack.rules {
                assert_eq!(heard.to_lowercase(), *heard, "{name}: {heard}");
                assert!(
                    heard.trim() == *heard && !heard.is_empty(),
                    "{name}: {heard:?}"
                );
                assert_ne!(heard, written, "{name}: {heard}");
                assert!(seen.insert(*heard), "{name}: {heard} twice");
            }
        }
    }

    #[test]
    fn no_casing_word_is_an_ordinary_word() {
        for (name, pack) in every_pack() {
            for word in pack.words.iter().filter(|word| is_casing_word(word)) {
                let lower = word.to_lowercase();
                assert!(!ORDINARY.contains(&lower.as_str()), "{name}: {word}");
            }
        }
    }

    #[test]
    fn casing_words_are_single_letter_led_words_with_a_later_capital() {
        assert!(is_casing_word("GitHub"));
        assert!(is_casing_word("EC2"));
        assert!(!is_casing_word("Python"));
        assert!(!is_casing_word("VS Code"));
        assert!(!is_casing_word(".NET"));
    }
}
