// FilePath: crates/sv-text/src/vocabulary.rs
//! Turns the personal dictionary and the built-in vocabulary (the product's name and the packs
//! the user picked) into what dictation needs: the Whisper spelling-bias prompt, the preferred
//! spellings handed to the formatting pass, the deterministic rewrite rules, and the names the
//! sound-alike pass and Parakeet's booster snap misheard words to.
//!
//! Prompt priority, highest first: the product's name, the personal dictionary in its own
//! order, then the packs, interleaved word by word so every enabled pack gets its best words
//! in. The personal dictionary wins every conflict: a phrase the user has, as a word or as the
//! heard side of a rule, gets no built-in rule and no built-in prompt word.

use std::collections::HashSet;

use sv_domain::{DictionaryEntry, VocabularyPack};

use crate::language_prompts;
use crate::packs::{self, is_casing_word, Pack, PRODUCT};
use crate::sound_alike::Spelling;
use crate::tokens::estimate_tokens;

/// Bounds the prompt in characters alongside the token estimate: about what 200 tokens of
/// English span, so neither limit lets the other run away on unusual words.
pub const PROMPT_MAX_CHARS: usize = 850;
/// Whisper keeps only the last 223 prompt tokens. The estimate runs high over a whole prompt
/// but can be one token short on a single word, so the cap leaves headroom.
pub const PROMPT_MAX_TOKENS: usize = 200;
/// The most of the prompt the packs may take. A couple of dozen well-known terms is where they
/// stop paying for the room they use, while their casing rules cover every word regardless.
pub const PACK_PROMPT_MAX_TOKENS: usize = 80;

/// Whisper tokenizes a word after ", " in fewer tokens than one glued to a bare comma.
const SEPARATOR: &str = ", ";

/// `from` (as heard, matched case-insensitively on word frontiers) is written as `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocabulary {
    /// Prompt words in priority order, as many as fit under [`PROMPT_MAX_CHARS`] and
    /// [`PROMPT_MAX_TOKENS`] joined by ", ". Also the formatting pass's preferred spellings.
    pub terms: Vec<String>,
    /// Replacement and casing rules, longest phrase first; among equal lengths the user's
    /// come before built-in ones.
    pub rules: Vec<Rule>,
    /// Personal dictionary words left out of the prompt because it was full.
    pub omitted: usize,
    /// Personal dictionary entries written as names, in dictionary order: what misheard words
    /// snap to (see [`crate::sound_alike`]). Built-in pack words are never targets.
    pub spellings: Vec<Spelling>,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::new(&[], &[])
    }
}

impl Vocabulary {
    pub fn new(entries: &[DictionaryEntry], packs: &[VocabularyPack]) -> Vocabulary {
        let user_phrases: HashSet<String> = entries
            .iter()
            .map(|entry| entry.phrase.trim().to_lowercase())
            .filter(|phrase| !phrase.is_empty())
            .collect();
        let mut prompt = Prompt::default();
        for word in PRODUCT.words {
            prompt.push(word, PROMPT_MAX_TOKENS);
        }

        let mut user_rules = Vec::new();
        let mut omitted = 0;
        let mut spellings = Vec::new();
        for entry in entries {
            let phrase = entry.phrase.trim();
            if phrase.is_empty() {
                continue;
            }
            if let Some(replacement) = &entry.replacement {
                user_rules.push(Rule {
                    from: phrase.to_owned(),
                    to: replacement.trim().to_owned(),
                });
                continue;
            }
            spellings.extend(Spelling::new(phrase));
            // Once one word does not fit, every later word is omitted too, so the prompt keeps
            // the dictionary's own priority order instead of back-filling with short words.
            if omitted > 0 || !prompt.push(phrase, PROMPT_MAX_TOKENS) {
                omitted += 1;
            }

            // An internal capital (ArgoCD, ShadCN, ESOPs) is unambiguous brand casing, so any
            // case the model returns is snapped to it. A word capitalised only on its first
            // letter is left alone: forcing "Frontend" mid-sentence would be wrong, and an
            // explicit rule covers the cases that need it.
            if has_internal_capital(phrase) {
                user_rules.push(Rule {
                    from: phrase.to_owned(),
                    to: phrase.to_owned(),
                });
            }
        }

        let chosen = unique_packs(packs);
        let builtin_rules = builtin_rules(&chosen, &user_phrases);
        let pack_limit = PROMPT_MAX_TOKENS.min(prompt.size.tokens + PACK_PROMPT_MAX_TOKENS);
        for word in interleaved(&chosen) {
            if user_phrases.contains(&word.to_lowercase()) {
                continue;
            }
            if !prompt.push(word, pack_limit) {
                break;
            }
        }

        // Longest phrase first, so "git status" wins over a "git" rule. The sort is stable,
        // which keeps the user's rules ahead and dictionary order among equal lengths.
        let mut rules = user_rules;
        rules.extend(builtin_rules);
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.from.chars().count()));

        Vocabulary {
            terms: prompt.terms,
            rules,
            omitted,
            spellings,
        }
    }

    /// The single-word names Parakeet's dictionary booster checks the audio for. Multi-word
    /// entries and ordinary English words ("Agents", "Graphics") are left out: the booster's
    /// acoustic match overreaches on them, rewriting "and" into the second word of a name and
    /// everyday words into an entry that sounds a little like them.
    pub fn boost_terms(&self) -> Vec<String> {
        self.spellings
            .iter()
            .filter(|spelling| spelling.is_single_word() && !spelling.is_ordinary_word())
            .map(|spelling| spelling.written.clone())
            .collect()
    }

    /// Appends terms read from the focused app (see [`crate::context`]) for this dictation only,
    /// after the dictionary and pack words, so they are the first to go when the prompt is full.
    /// A term already listed in any case, or the heard side of a replacement rule, is skipped.
    /// Terms stop at the first one that no longer fits [`PROMPT_MAX_TOKENS`] and
    /// [`PROMPT_MAX_CHARS`], keeping the caller's priority order.
    pub fn add_context_terms(&mut self, terms: &[String]) {
        let mut prompt = Prompt::default();
        for term in &self.terms {
            prompt.push(term, usize::MAX);
        }
        let heard: HashSet<String> = self
            .rules
            .iter()
            .filter(|rule| rule.from != rule.to)
            .map(|rule| rule.from.to_lowercase())
            .collect();
        for term in terms {
            let term = term.trim();
            if term.is_empty() || heard.contains(&term.to_lowercase()) {
                continue;
            }
            if !prompt.push(term, PROMPT_MAX_TOKENS) {
                break;
            }
        }
        self.terms = prompt.terms;
    }
}

/// Whisper `prompt`: the lead-in of each selected language, then the vocabulary words joined by
/// ", ", within [`PROMPT_MAX_CHARS`] and [`PROMPT_MAX_TOKENS`]. Whisper continues in the style of
/// its prompt, so the lead keeps a language in the script the user chose. Words that no longer
/// fit beside the lead are dropped from the end, the lowest priority first.
pub fn whisper_prompt(languages: &[String], vocabulary: &Vocabulary) -> String {
    let mut prompt = String::new();
    let mut size = Size::default();
    for lead in language_prompts::whisper_leads(languages) {
        let next = size.plus(Size::of(lead));
        if !next.within(PROMPT_MAX_TOKENS) {
            break;
        }
        prompt.push_str(lead);
        size = next;
    }
    for (index, term) in vocabulary.terms.iter().enumerate() {
        let next = size.plus(Size::term(term, index == 0));
        if !next.within(PROMPT_MAX_TOKENS) {
            break;
        }
        if index > 0 {
            prompt.push_str(SEPARATOR);
        }
        prompt.push_str(term);
        size = next;
    }
    prompt
}

#[derive(Debug, Clone, Copy, Default)]
struct Size {
    chars: usize,
    tokens: usize,
}

impl Size {
    fn of(text: &str) -> Size {
        Size {
            chars: text.chars().count(),
            tokens: estimate_tokens(text),
        }
    }

    /// What appending `term` to a ", "-joined list costs.
    fn term(term: &str, first: bool) -> Size {
        let word = Size::of(term);
        if first {
            word
        } else {
            word.plus(Size::of(SEPARATOR))
        }
    }

    fn plus(self, other: Size) -> Size {
        Size {
            chars: self.chars + other.chars,
            tokens: self.tokens + other.tokens,
        }
    }

    fn within(self, max_tokens: usize) -> bool {
        self.chars <= PROMPT_MAX_CHARS && self.tokens <= max_tokens
    }
}

/// The prompt word list as it is built, each word once whatever its case.
#[derive(Default)]
struct Prompt {
    terms: Vec<String>,
    listed: HashSet<String>,
    size: Size,
}

impl Prompt {
    /// Appends `term` if the list stays within `max_tokens`; a word already listed counts as
    /// fitting. Returns whether it fit.
    fn push(&mut self, term: &str, max_tokens: usize) -> bool {
        let key = term.to_lowercase();
        if self.listed.contains(&key) {
            return true;
        }
        let next = self.size.plus(Size::term(term, self.terms.is_empty()));
        if !next.within(max_tokens) {
            return false;
        }
        self.size = next;
        self.terms.push(term.to_owned());
        self.listed.insert(key);
        true
    }
}

fn unique_packs(packs: &[VocabularyPack]) -> Vec<&'static Pack> {
    let mut seen: Vec<VocabularyPack> = Vec::with_capacity(packs.len());
    for &id in packs {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen.into_iter().map(packs::pack).collect()
}

/// The packs' words in turns: every pack's first word, then every pack's second, and so on.
fn interleaved<'a>(packs: &'a [&'static Pack]) -> impl Iterator<Item = &'static str> + 'a {
    let longest = packs.iter().map(|pack| pack.words.len()).max().unwrap_or(0);
    (0..longest).flat_map(move |index| {
        packs
            .iter()
            .filter_map(move |pack| pack.words.get(index).copied())
    })
}

/// Casing rules of the product's name and every pack word (prompt room does not limit them),
/// then the packs' replacement rules, each heard phrase once and none the user already has.
fn builtin_rules(packs: &[&'static Pack], user_phrases: &HashSet<String>) -> Vec<Rule> {
    let mut taken = user_phrases.clone();
    let mut rules = Vec::new();
    let casing = PRODUCT
        .words
        .iter()
        .copied()
        .chain(interleaved(packs))
        .filter(|word| is_casing_word(word))
        .map(|word| (word, word));
    let replacements = packs.iter().flat_map(|pack| pack.rules.iter().copied());
    for (heard, written) in casing.chain(replacements) {
        if taken.insert(heard.to_lowercase()) {
            rules.push(Rule {
                from: heard.to_owned(),
                to: written.to_owned(),
            });
        }
    }
    rules
}

fn has_internal_capital(phrase: &str) -> bool {
    phrase.chars().skip(1).any(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatting::format;
    use sv_domain::{DictionarySource, Style};

    fn word(phrase: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: 0,
            phrase: phrase.to_owned(),
            replacement: None,
            created_at: 0,
            source: DictionarySource::Manual,
        }
    }

    fn rule(phrase: &str, replacement: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: 0,
            phrase: phrase.to_owned(),
            replacement: Some(replacement.to_owned()),
            created_at: 0,
            source: DictionarySource::Manual,
        }
    }

    fn languages(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    fn tokens(prompt: &str) -> usize {
        estimate_tokens(prompt)
    }

    const LEAD: &str = "Haan, toh main ab yeh code check karta hoon. ";
    const PRODUCT_TERMS: &str = "Simple Voice, SimpleVoice";

    #[test]
    fn prompt_is_the_selected_leads_then_the_product_name_then_the_dictionary() {
        let vocab = Vocabulary::new(
            &[word("Tauri"), word("sqlx"), rule("btw", "by the way")],
            &[],
        );
        assert_eq!(
            vocab.terms,
            vec!["Simple Voice", "SimpleVoice", "Tauri", "sqlx"]
        );
        assert_eq!(vocab.omitted, 0);
        let english = format!("{PRODUCT_TERMS}, Tauri, sqlx");
        assert_eq!(whisper_prompt(&languages(&["en"]), &vocab), english);
        assert_eq!(
            whisper_prompt(&languages(&["en", "hi-Latn"]), &vocab),
            format!("{LEAD}{english}")
        );
        assert_eq!(
            whisper_prompt(&languages(&["hi-Latn"]), &Vocabulary::default()),
            format!("{LEAD}{PRODUCT_TERMS}")
        );
    }

    #[test]
    fn dictionary_words_stop_at_the_token_cap_and_count_every_later_word() {
        // Each "kubernetesN" costs 6 estimated tokens with its separator.
        let entries: Vec<DictionaryEntry> =
            (0..40).map(|n| word(&format!("kubernetes{n}"))).collect();
        let vocab = Vocabulary::new(&entries, &[]);
        let prompt = whisper_prompt(&languages(&["en"]), &vocab);
        assert!(tokens(&prompt) <= PROMPT_MAX_TOKENS);
        assert!(tokens(&prompt) > PROMPT_MAX_TOKENS - 8);
        assert_eq!(vocab.terms.len() - 2 + vocab.omitted, 40);
        assert!(vocab.omitted > 0);
    }

    #[test]
    fn a_word_that_does_not_fit_omits_every_later_one_even_short_ones() {
        let long = "x".repeat(600);
        let vocab = Vocabulary::new(&[word(&long), word(&"y".repeat(200)), word("z")], &[]);
        assert_eq!(vocab.terms, vec!["Simple Voice", "SimpleVoice"]);
        assert_eq!(vocab.omitted, 3);
    }

    #[test]
    fn words_that_no_longer_fit_beside_the_lead_are_dropped_from_the_end() {
        let entries: Vec<DictionaryEntry> = (0..60).map(|n| word(&format!("w{n}"))).collect();
        let vocab = Vocabulary::new(&entries, &[]);
        let without = whisper_prompt(&languages(&["en"]), &vocab);
        let with = whisper_prompt(&languages(&["hi-Latn"]), &vocab);
        assert!(with.starts_with(LEAD));
        assert!(tokens(&with) <= PROMPT_MAX_TOKENS);
        let kept = with.trim_start_matches(LEAD);
        assert!(without.starts_with(kept) && kept.len() < without.len());
    }

    #[test]
    fn rules_are_sorted_longest_first_and_only_internal_caps_words_become_rules() {
        let vocab = Vocabulary::new(
            &[
                rule("git", "Git"),
                word("ArgoCD"),
                word("Frontend"),
                rule("git status", "`git status`"),
            ],
            &[],
        );
        let heard: Vec<&str> = vocab.rules.iter().map(|r| r.from.as_str()).collect();
        assert_eq!(heard, vec!["SimpleVoice", "git status", "ArgoCD", "git"]);
        assert_eq!(vocab.terms[2..], ["ArgoCD", "Frontend"]);
    }

    #[test]
    fn the_product_name_is_cased_only_when_written_as_one_word() {
        let vocab = Vocabulary::default();
        let out = format(
            "i use simplevoice for a simple voice memo",
            &vocab,
            Style::Formal,
        );
        assert_eq!(out.text, "I use SimpleVoice for a simple voice memo");
    }

    #[test]
    fn packs_fix_casing_and_spoken_forms_of_every_word_not_only_the_prompted_ones() {
        let packs = [VocabularyPack::Programming, VocabularyPack::Web];
        let vocab = Vocabulary::new(&[], &packs);
        assert!(!vocab.terms.iter().any(|term| term == "PowerShell"));
        let out = format(
            "deploy the node js app from github with powershell",
            &vocab,
            Style::Formal,
        );
        assert_eq!(
            out.text,
            "Deploy the Node.js app from GitHub with PowerShell"
        );
    }

    #[test]
    fn pack_words_follow_the_dictionary_take_turns_and_stay_under_their_share() {
        let packs = [VocabularyPack::Programming, VocabularyPack::Everyday];
        let vocab = Vocabulary::new(&[word("Tauri")], &packs);
        assert_eq!(
            vocab.terms[..6],
            [
                "Simple Voice",
                "SimpleVoice",
                "Tauri",
                "TypeScript",
                "ChatGPT",
                "JavaScript"
            ]
        );
        let user = tokens(&format!("{PRODUCT_TERMS}, Tauri"));
        assert!(tokens(&vocab.terms.join(SEPARATOR)) <= user + PACK_PROMPT_MAX_TOKENS);
    }

    #[test]
    fn a_users_phrase_overrides_the_built_in_word_and_rules_for_it() {
        let packs = [VocabularyPack::Programming, VocabularyPack::Web];
        let vocab = Vocabulary::new(&[word("Github"), rule("node js", "Node")], &packs);
        assert!(!vocab.terms.iter().any(|term| term == "GitHub"));
        let out = format("push node js code to github", &vocab, Style::Formal);
        assert_eq!(out.text, "Push Node code to github");
    }

    #[test]
    fn packs_never_turn_a_multi_word_or_symbol_led_spelling_into_a_casing_rule() {
        let vocab = Vocabulary::new(&[], &[VocabularyPack::Backend]);
        let out = format(
            "see example.net and the spring boot docs",
            &vocab,
            Style::Formal,
        );
        assert_eq!(out.text, "See example.net and the spring boot docs");
    }

    #[test]
    fn a_pack_listed_twice_counts_once() {
        let once = Vocabulary::new(&[], &[VocabularyPack::Sales]);
        let twice = Vocabulary::new(&[], &[VocabularyPack::Sales, VocabularyPack::Sales]);
        assert_eq!(once, twice);
    }

    #[test]
    fn context_terms_follow_the_dictionary_once_and_never_as_a_heard_phrase() {
        let mut vocab = Vocabulary::new(&[word("Tauri"), rule("btw", "by the way")], &[]);
        let context = languages(&["tauri", " Priya ", "BTW", "", "priya", "KubeCon"]);
        vocab.add_context_terms(&context);
        assert_eq!(
            vocab.terms,
            vec!["Simple Voice", "SimpleVoice", "Tauri", "Priya", "KubeCon"]
        );
        assert_eq!(
            whisper_prompt(&languages(&["en"]), &vocab),
            format!("{PRODUCT_TERMS}, Tauri, Priya, KubeCon")
        );
    }

    #[test]
    fn context_terms_stop_at_the_prompt_budget_in_their_given_order() {
        let mut vocab = Vocabulary::new(&[word("Tauri")], &[]);
        let context: Vec<String> = (0..100).map(|n| format!("Identifier{n}")).collect();
        vocab.add_context_terms(&context);
        let added: Vec<&String> = vocab.terms.iter().skip(3).collect();
        assert!(!added.is_empty());
        assert!(added.len() < context.len());
        assert!(added
            .iter()
            .zip(&context)
            .all(|(term, given)| *term == given));
        let prompt = whisper_prompt(&languages(&["en"]), &vocab);
        assert!(prompt.chars().count() <= PROMPT_MAX_CHARS);
        assert!(tokens(&prompt) <= PROMPT_MAX_TOKENS);
        assert!(prompt.ends_with(added.last().map_or("", |term| term.as_str())));
    }
}
