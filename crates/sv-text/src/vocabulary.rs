// FilePath: crates/sv-text/src/vocabulary.rs
//! Turns the personal dictionary into the three things dictation needs: the Whisper
//! spelling-bias prompt, the preferred spellings handed to the formatting pass, and the
//! deterministic rewrite rules.

use sv_domain::DictionaryEntry;

use crate::language_prompts;

/// Groq rejects prompts above 896 characters; the cap leaves headroom.
pub const PROMPT_MAX_CHARS: usize = 850;

/// `from` (as heard, matched case-insensitively on word frontiers) is written as `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vocabulary {
    /// Dictionary words for the Whisper prompt, in dictionary order, as many as fit in
    /// [`PROMPT_MAX_CHARS`] characters joined by commas.
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
        let mut prompt_chars = 0;
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
            terms,
            rules,
            omitted,
        }
    }
}

/// Whisper `prompt`: the lead-in of each selected language, then the dictionary words joined by
/// commas, at most [`PROMPT_MAX_CHARS`] characters. Whisper continues in the style of its
/// prompt, so the lead keeps a language in the script the user chose. Words that no longer fit
/// beside the lead are dropped from the end, keeping the dictionary's priority order.
pub fn whisper_prompt(languages: &[String], vocabulary: &Vocabulary) -> String {
    let mut prompt = String::new();
    let mut chars = 0;
    for lead in language_prompts::selected(languages).filter_map(|lang| lang.whisper_lead) {
        let added = lead.chars().count();
        if chars + added > PROMPT_MAX_CHARS {
            break;
        }
        prompt.push_str(lead);
        chars += added;
    }
    for (index, term) in vocabulary.terms.iter().enumerate() {
        let separator = if index == 0 { "" } else { "," };
        let added = separator.len() + term.chars().count();
        if chars + added > PROMPT_MAX_CHARS {
            break;
        }
        prompt.push_str(separator);
        prompt.push_str(term);
        chars += added;
    }
    prompt
}

fn has_internal_capital(phrase: &str) -> bool {
    phrase.chars().skip(1).any(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_domain::DictionarySource;

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

    const LEAD: &str = "Haan, toh main ab yeh code check karta hoon. ";

    #[test]
    fn whisper_prompt_joins_words_with_commas_after_the_selected_leads_only() {
        let vocab =
            Vocabulary::from_entries(&[word("Tauri"), word("sqlx"), rule("btw", "by the way")]);
        assert_eq!(vocab.terms, vec!["Tauri", "sqlx"]);
        assert_eq!(vocab.omitted, 0);
        assert_eq!(whisper_prompt(&languages(&["en"]), &vocab), "Tauri,sqlx");
        assert_eq!(
            whisper_prompt(&languages(&["en", "hi-Latn"]), &vocab),
            format!("{LEAD}Tauri,sqlx")
        );
        assert_eq!(
            whisper_prompt(&languages(&["hi-Latn"]), &Vocabulary::default()),
            LEAD
        );
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
        assert_eq!(vocab.terms, vec![long.clone()]);
        assert_eq!(vocab.omitted, 3);
        assert_eq!(whisper_prompt(&languages(&["en"]), &vocab), long);
    }

    #[test]
    fn cap_counts_characters_not_bytes() {
        let devanagari = "क".repeat(PROMPT_MAX_CHARS);
        let vocab = Vocabulary::from_entries(&[word(&devanagari)]);
        assert_eq!(vocab.omitted, 0);
        let prompt = whisper_prompt(&languages(&["hi"]), &vocab);
        assert_eq!(prompt.chars().count(), PROMPT_MAX_CHARS);
    }

    #[test]
    fn words_that_no_longer_fit_beside_the_lead_are_dropped_from_the_end() {
        let lead = LEAD.chars().count();
        let first = "a".repeat(PROMPT_MAX_CHARS - lead - 2);
        let vocab = Vocabulary::from_entries(&[word(&first), word("b"), word("cc")]);
        assert_eq!(vocab.terms.len(), 3);
        let prompt = whisper_prompt(&languages(&["hi-Latn"]), &vocab);
        assert_eq!(prompt, format!("{LEAD}{first},b"));
        assert_eq!(prompt.chars().count(), PROMPT_MAX_CHARS);
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
