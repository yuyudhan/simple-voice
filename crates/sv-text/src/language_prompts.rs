// FilePath: crates/sv-text/src/language_prompts.rs
//! Prompt content that only makes sense for one language, keyed by its registry tag: the
//! Whisper lead-in, extra fillers and the few-shot examples of the formatting pass. A tag's
//! content is sent only when the user selected that tag, so an English-only user never pays
//! for (or is confused by) another language's examples. Adding a language's prompt content is
//! one entry in [`PROMPTS`]; nothing else in the crate names a language.

use sv_domain::languages::{language, Language};

use crate::polish::{PolishTarget, Shot};

pub(crate) struct LanguagePrompt {
    /// Registry tag this content belongs to.
    tag: &'static str,
    /// Sentence that opens the Whisper prompt. Whisper continues in the style of its prompt,
    /// so a sentence in the right script keeps the transcript in that script.
    pub(crate) whisper_lead: Option<&'static str>,
    /// Fillers the formatting pass drops, on top of the generic English ones.
    pub(crate) fillers: &'static [&'static str],
    pub(crate) formal_shots: &'static [Shot],
    pub(crate) casual_shots: &'static [Shot],
}

// The romanised Hindi shots pin what the rules alone did not hold in evaluation: mixed speech
// kept word for word, in Roman script. They go to chat models only, because the on-device Apple
// model rejects any request containing Hindi as "unsupported language".
const PROMPTS: &[LanguagePrompt] = &[
    LanguagePrompt {
        tag: "hi",
        // Without it Whisper Turbo writes most English words in Hindi speech in Devanagari
        // ("मीटिंग"); with it it keeps them in Roman script, as Hindi mode promises.
        whisper_lead: Some("ठीक है, मैं अभी email पढ़कर reply भेजता हूँ। "),
        fillers: &[],
        formal_shots: &[],
        casual_shots: &[],
    },
    LanguagePrompt {
        tag: "hi-Latn",
        // Without it Whisper flips mixed Hindi-English speech to Devanagari.
        whisper_lead: Some("Haan, toh main ab yeh code check karta hoon. "),
        fillers: &["matlab", "na"],
        formal_shots: &[Shot {
            user: "yaar ye wala query actually bahut slow hai, matlab we need to add an index \
first, phir deploy karenge",
            reply: "Ye wala query bahut slow hai; we need to add an index first, phir deploy \
karenge.",
            only: Some(PolishTarget::Chat),
        }],
        casual_shots: &[Shot {
            user: "um yaar ye build phir se fail ho gaya, basically I think we need to clear the \
cache pehle",
            reply: "Yaar ye build phir se fail ho gaya, I think we need to clear the cache pehle",
            only: Some(PolishTarget::Chat),
        }],
    },
];

/// Prompt content of the selected tags, in selection order, each tag once.
pub(crate) fn selected(languages: &[String]) -> impl Iterator<Item = &'static LanguagePrompt> + '_ {
    let mut seen: Vec<&'static str> = Vec::new();
    languages.iter().filter_map(move |tag| {
        let tag = tag.trim();
        let prompt = PROMPTS
            .iter()
            .find(|prompt| prompt.tag.eq_ignore_ascii_case(tag))?;
        if seen.contains(&prompt.tag) {
            return None;
        }
        seen.push(prompt.tag);
        Some(prompt)
    })
}

/// Whisper leads of the selected tags, in selection order. A language whose romanised variant
/// is selected too gives up its lead: Smart Select writes the variant offline then, and leads in
/// two scripts would pull Whisper both ways.
pub(crate) fn whisper_leads(languages: &[String]) -> impl Iterator<Item = &'static str> + '_ {
    let variant_selected = |base: &str| {
        languages
            .iter()
            .filter_map(|tag| language(tag))
            .any(|other| other.is_variant() && other.base == base)
    };
    selected(languages).filter_map(move |prompt| {
        let own = language(prompt.tag)?;
        let outvoted = !own.is_variant() && variant_selected(own.base);
        if outvoted {
            None
        } else {
            prompt.whisper_lead
        }
    })
}

/// One line telling the model which script each selected language is written in, e.g.
/// "Writing: Hindi in Devanagari; English in Roman script." Only sent when it carries
/// information: two selected languages differ in script, or one is written in a script other
/// than its own. Unknown tags are skipped.
pub(crate) fn writing_line(languages: &[String]) -> Option<String> {
    let mut known: Vec<&'static Language> = Vec::new();
    for found in languages.iter().filter_map(|tag| language(tag)) {
        if !known.iter().any(|seen| seen.tag == found.tag) {
            known.push(found);
        }
    }
    let first = known.first()?;
    let informative = known
        .iter()
        .any(|lang| lang.is_variant() || lang.script != first.script);
    if !informative {
        return None;
    }
    let parts: Vec<String> = known
        .iter()
        .map(|lang| format!("{} in {}", lang.name, lang.script.user_name()))
        .collect();
    Some(format!("Writing: {}.", parts.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn writing_line_names_each_selected_script_in_selection_order() {
        assert_eq!(
            writing_line(&tags(&["hi-Latn", "hi", "en"])).as_deref(),
            Some(
                "Writing: Hinglish in Roman script; Hindi in Devanagari; English in Roman script."
            )
        );
    }

    #[test]
    fn writing_line_is_omitted_when_every_language_uses_its_own_shared_script() {
        assert_eq!(writing_line(&tags(&["en"])), None);
        assert_eq!(writing_line(&tags(&["en", "de", "fr"])), None);
        assert_eq!(writing_line(&tags(&[])), None);
    }

    #[test]
    fn writing_line_is_sent_for_a_lone_variant_and_skips_unknown_tags() {
        assert_eq!(
            writing_line(&tags(&["xx-Nope", "hi-Latn", "hi-latn"])).as_deref(),
            Some("Writing: Hinglish in Roman script.")
        );
        assert_eq!(
            writing_line(&tags(&["en", "hi"])).as_deref(),
            Some("Writing: English in Roman script; Hindi in Devanagari.")
        );
    }

    #[test]
    fn content_is_selected_by_tag_once_regardless_of_case() {
        let found: Vec<&str> = selected(&tags(&["en", " HI-latn", "hi-Latn"]))
            .map(|prompt| prompt.tag)
            .collect();
        assert_eq!(found, vec!["hi-Latn"]);
        assert_eq!(selected(&tags(&["en", "de"])).count(), 0);
    }

    #[test]
    fn a_language_gives_up_its_lead_to_its_selected_romanised_variant() {
        let leads = |selected: &[&str]| whisper_leads(&tags(selected)).collect::<Vec<_>>();
        let hindi = "ठीक है, मैं अभी email पढ़कर reply भेजता हूँ। ";
        let hinglish = "Haan, toh main ab yeh code check karta hoon. ";
        assert_eq!(leads(&["en", "hi"]), vec![hindi]);
        assert_eq!(leads(&["hi-Latn"]), vec![hinglish]);
        assert_eq!(leads(&["en", "hi", "hi-Latn"]), vec![hinglish]);
        assert_eq!(leads(&["hi-Latn", "hi"]), vec![hinglish]);
        assert!(leads(&["en"]).is_empty());
    }
}
