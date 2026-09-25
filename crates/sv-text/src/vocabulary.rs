// FilePath: crates/sv-text/src/vocabulary.rs
//! Turns the personal dictionary into the three things dictation needs: the Whisper
//! spelling-bias prompt, the preferred spellings handed to the formatting pass, and the
//! deterministic rewrite rules.

use sv_domain::DictionaryEntry;

/// Groq rejects prompts above 896 characters; the cap leaves headroom.
pub const PROMPT_MAX_CHARS: usize = 850;

/// A romanized Hinglish lead-in keeps mixed speech in Latin script instead of letting Whisper
/// flip the whole clip to Devanagari.
const PROMPT_LEAD: &str = "Haan, toh main ab yeh code check karta hoon. ";

/// `from` (as heard, matched case-insensitively on word frontiers) is written as `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocabulary {
    /// Whisper `prompt`: the lead-in followed by dictionary words, at most
    /// [`PROMPT_MAX_CHARS`] characters.
    pub prompt: String,
    /// Words that made it into the prompt, in dictionary order.
    pub terms: Vec<String>,
    /// Replacement rules plus brand-casing rules, longest phrase first.
    pub rules: Vec<Rule>,
    /// Words left out of the prompt because it was full.
    pub omitted: usize,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self::from_entries(&[])
    }
}

impl Vocabulary {
    pub fn from_entries(entries: &[DictionaryEntry]) -> Vocabulary {
        let mut prompt = PROMPT_LEAD.to_owned();
        let mut prompt_chars = prompt.chars().count();
        let mut terms: Vec<String> = Vec::new();
        let mut rules = Vec::new();
        let mut omitted = 0;

        for entry in entries {
            let phrase = entry.phrase.trim();
            if phrase.is_empty() {
                continue;
            }
            if let Some(replacement) = &entry.replacement {
                rules.push(Rule {
                    from: phrase.to_owned(),
                    to: replacement.trim().to_owned(),
                });
                continue;
            }

            // Once one word does not fit, every later word is omitted too, so the prompt keeps
            // the dictionary's own priority order instead of back-filling with short words.
            let separator = if terms.is_empty() { "" } else { "," };
            let added = separator.len() + phrase.chars().count();
            if omitted == 0 && prompt_chars + added <= PROMPT_MAX_CHARS {
                prompt.push_str(separator);
                prompt.push_str(phrase);
                prompt_chars += added;
                terms.push(phrase.to_owned());
            } else {
                omitted += 1;
            }

            // An internal capital (ArgoCD, ShadCN, ESOPs) is unambiguous brand casing, so any
            // case the model returns is snapped to it. A word capitalised only on its first
            // letter is left alone: forcing "Frontend" mid-sentence would be wrong, and an
            // explicit rule covers the cases that need it.
            if has_internal_capital(phrase) {
                rules.push(Rule {
                    from: phrase.to_owned(),
                    to: phrase.to_owned(),
                });
            }
        }

        // Longest phrase first, so "git status" wins over a "git" rule. The sort is stable,
        // which keeps dictionary order among equal lengths.
        rules.sort_by_key(|rule| std::cmp::Reverse(rule.from.chars().count()));

        Vocabulary {
            prompt,
            terms,
            rules,
            omitted,
        }
    }
}

fn has_internal_capital(phrase: &str) -> bool {
    phrase.chars().skip(1).any(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(phrase: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: 0,
            phrase: phrase.to_owned(),
            replacement: None,
            created_at: 0,
        }
    }

    fn rule(phrase: &str, replacement: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: 0,
            phrase: phrase.to_owned(),
            replacement: Some(replacement.to_owned()),
            created_at: 0,
        }
    }

    #[test]
    fn prompt_starts_with_the_hinglish_lead_and_joins_words_with_commas() {
        let vocab =
            Vocabulary::from_entries(&[word("Tauri"), word("sqlx"), rule("btw", "by the way")]);
        assert_eq!(
            vocab.prompt,
            "Haan, toh main ab yeh code check karta hoon. Tauri,sqlx"
        );
        assert_eq!(vocab.terms, vec!["Tauri", "sqlx"]);
        assert_eq!(vocab.omitted, 0);
    }

    #[test]
    fn prompt_is_capped_and_counts_every_word_after_the_first_that_does_not_fit() {
        let long = "x".repeat(700);
        let entries = [
            word(&long),
            word(&"y".repeat(200)),
            word("z"),
            word("short"),
        ];
        let vocab = Vocabulary::from_entries(&entries);
        assert!(vocab.prompt.chars().count() <= PROMPT_MAX_CHARS);
        assert_eq!(vocab.terms, vec![long]);
        assert_eq!(vocab.omitted, 3);
    }

    #[test]
    fn cap_counts_characters_not_bytes() {
        let lead = PROMPT_LEAD.chars().count();
        let devanagari = "क".repeat(PROMPT_MAX_CHARS - lead);
        let vocab = Vocabulary::from_entries(&[word(&devanagari)]);
        assert_eq!(vocab.omitted, 0);
        assert_eq!(vocab.prompt.chars().count(), PROMPT_MAX_CHARS);
    }

    #[test]
    fn rules_are_sorted_longest_first_and_only_internal_caps_words_become_rules() {
        let vocab = Vocabulary::from_entries(&[
            rule("git", "Git"),
            word("ArgoCD"),
            word("Frontend"),
            rule("git status", "`git status`"),
        ]);
        let heard: Vec<&str> = vocab.rules.iter().map(|r| r.from.as_str()).collect();
        assert_eq!(heard, vec!["git status", "ArgoCD", "git"]);
        assert_eq!(vocab.terms, vec!["ArgoCD", "Frontend"]);
    }
}
