// FilePath: crates/sv-domain/src/languages.rs
//! The dictation languages the app offers. Every language-aware decision (validation, Smart
//! Select routing, the mixed-script retry, prompts, the language picker) reads this registry,
//! so a new language or a romanised variant of one is an entry here, not a code path.
//!
//! Tags are BCP 47: a plain ISO 639-1 code (`hi`) is the language in its own script, and a
//! script subtag (`hi-Latn`) is the same spoken language written in another script. Models only
//! know the spoken language, so they receive [`Language::base`].

use serde::Serialize;

/// A writing system, as far as the app needs to tell them apart in a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Devanagari,
    Arabic,
    Bengali,
    Cyrillic,
    Greek,
    Gujarati,
    Gurmukhi,
    Hebrew,
    Kannada,
    Malayalam,
    Tamil,
    Telugu,
    Thai,
    Han,
    Japanese,
    Hangul,
}

impl Script {
    pub fn name(self) -> &'static str {
        match self {
            Self::Latin => "Latin",
            Self::Devanagari => "Devanagari",
            Self::Arabic => "Arabic",
            Self::Bengali => "Bengali",
            Self::Cyrillic => "Cyrillic",
            Self::Greek => "Greek",
            Self::Gujarati => "Gujarati",
            Self::Gurmukhi => "Gurmukhi",
            Self::Hebrew => "Hebrew",
            Self::Kannada => "Kannada",
            Self::Malayalam => "Malayalam",
            Self::Tamil => "Tamil",
            Self::Telugu => "Telugu",
            Self::Thai => "Thai",
            Self::Han => "Han",
            Self::Japanese => "Japanese",
            Self::Hangul => "Hangul",
        }
    }

    /// The name the UI shows. Speakers of romanised languages call Latin "Roman script".
    pub fn user_name(self) -> &'static str {
        match self {
            Self::Latin => "Roman script",
            other => other.name(),
        }
    }

    /// Whether `c` is a letter of this script: the Unicode blocks its letters live in.
    pub fn contains(self, c: char) -> bool {
        let ranges: &[(char, char)] = match self {
            // ASCII letters plus the accented letters of Latin-1 Supplement and Extended-A;
            // Latin-1's × and ÷ are symbols, not letters.
            Self::Latin => {
                return c.is_ascii_alphabetic()
                    || (('\u{00C0}'..='\u{017F}').contains(&c) && c != '×' && c != '÷');
            }
            Self::Devanagari => &[('\u{0900}', '\u{097F}'), ('\u{A8E0}', '\u{A8FF}')],
            Self::Arabic => &[
                ('\u{0600}', '\u{06FF}'),
                ('\u{0750}', '\u{077F}'),
                ('\u{FB50}', '\u{FDFF}'),
                ('\u{FE70}', '\u{FEFF}'),
            ],
            Self::Bengali => &[('\u{0980}', '\u{09FF}')],
            Self::Cyrillic => &[('\u{0400}', '\u{052F}')],
            Self::Greek => &[('\u{0370}', '\u{03FF}'), ('\u{1F00}', '\u{1FFF}')],
            Self::Gujarati => &[('\u{0A80}', '\u{0AFF}')],
            Self::Gurmukhi => &[('\u{0A00}', '\u{0A7F}')],
            Self::Hebrew => &[('\u{0590}', '\u{05FF}')],
            Self::Kannada => &[('\u{0C80}', '\u{0CFF}')],
            Self::Malayalam => &[('\u{0D00}', '\u{0D7F}')],
            Self::Tamil => &[('\u{0B80}', '\u{0BFF}')],
            Self::Telugu => &[('\u{0C00}', '\u{0C7F}')],
            Self::Thai => &[('\u{0E00}', '\u{0E7F}')],
            Self::Han => HAN,
            // Japanese is written in kana and kanji, so Han letters count too.
            Self::Japanese => &[
                ('\u{3040}', '\u{30FF}'),
                ('\u{31F0}', '\u{31FF}'),
                ('\u{FF66}', '\u{FF9F}'),
                ('\u{3400}', '\u{4DBF}'),
                ('\u{4E00}', '\u{9FFF}'),
                ('\u{F900}', '\u{FAFF}'),
            ],
            Self::Hangul => &[
                ('\u{1100}', '\u{11FF}'),
                ('\u{3130}', '\u{318F}'),
                ('\u{AC00}', '\u{D7AF}'),
            ],
        };
        ranges
            .iter()
            .any(|&(first, last)| (first..=last).contains(&c))
    }
}

const HAN: &[(char, char)] = &[
    ('\u{3400}', '\u{4DBF}'),
    ('\u{4E00}', '\u{9FFF}'),
    ('\u{F900}', '\u{FAFF}'),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// BCP 47 tag stored in settings: `en`, `hi`, `hi-Latn`.
    pub tag: &'static str,
    /// English name shown in the UI.
    pub name: &'static str,
    /// ISO 639-1 code speech models (Groq, Whisper, Apple Speech) receive.
    pub base: &'static str,
    /// How the app writes this language.
    pub script: Script,
    /// The base language's own script.
    pub native: Script,
    /// Offered as a one-click chip in the language picker.
    pub primary: bool,
}

impl Language {
    /// A spoken language written in a script other than its own, e.g. romanised Hindi.
    pub fn is_variant(&self) -> bool {
        self.script != self.native
    }
}

const fn native(tag: &'static str, name: &'static str, script: Script) -> Language {
    Language {
        tag,
        name,
        base: tag,
        script,
        native: script,
        primary: false,
    }
}

const fn primary(language: Language) -> Language {
    Language {
        primary: true,
        ..language
    }
}

/// Every language the app offers: the language picker's list plus the rest of Parakeet v3's.
pub const LANGUAGES: &[Language] = &[
    primary(native("en", "English", Script::Latin)),
    primary(native("hi", "Hindi", Script::Devanagari)),
    Language {
        tag: "hi-Latn",
        name: "Hinglish",
        base: "hi",
        script: Script::Latin,
        native: Script::Devanagari,
        primary: true,
    },
    native("af", "Afrikaans", Script::Latin),
    native("ar", "Arabic", Script::Arabic),
    native("bg", "Bulgarian", Script::Cyrillic),
    native("bn", "Bengali", Script::Bengali),
    native("ca", "Catalan", Script::Latin),
    native("cs", "Czech", Script::Latin),
    native("da", "Danish", Script::Latin),
    native("de", "German", Script::Latin),
    native("el", "Greek", Script::Greek),
    native("es", "Spanish", Script::Latin),
    native("et", "Estonian", Script::Latin),
    native("fa", "Persian", Script::Arabic),
    native("fi", "Finnish", Script::Latin),
    native("fr", "French", Script::Latin),
    native("gu", "Gujarati", Script::Gujarati),
    native("he", "Hebrew", Script::Hebrew),
    native("hr", "Croatian", Script::Latin),
    native("hu", "Hungarian", Script::Latin),
    native("id", "Indonesian", Script::Latin),
    native("it", "Italian", Script::Latin),
    native("ja", "Japanese", Script::Japanese),
    native("kn", "Kannada", Script::Kannada),
    native("ko", "Korean", Script::Hangul),
    native("lt", "Lithuanian", Script::Latin),
    native("lv", "Latvian", Script::Latin),
    native("ml", "Malayalam", Script::Malayalam),
    native("mr", "Marathi", Script::Devanagari),
    native("ms", "Malay", Script::Latin),
    native("mt", "Maltese", Script::Latin),
    native("ne", "Nepali", Script::Devanagari),
    native("nl", "Dutch", Script::Latin),
    native("no", "Norwegian", Script::Latin),
    native("pa", "Punjabi", Script::Gurmukhi),
    native("pl", "Polish", Script::Latin),
    native("pt", "Portuguese", Script::Latin),
    native("ro", "Romanian", Script::Latin),
    native("ru", "Russian", Script::Cyrillic),
    native("sk", "Slovak", Script::Latin),
    native("sl", "Slovenian", Script::Latin),
    native("sr", "Serbian", Script::Cyrillic),
    native("sv", "Swedish", Script::Latin),
    native("sw", "Swahili", Script::Latin),
    native("ta", "Tamil", Script::Tamil),
    native("te", "Telugu", Script::Telugu),
    native("th", "Thai", Script::Thai),
    native("tl", "Tagalog", Script::Latin),
    native("tr", "Turkish", Script::Latin),
    native("uk", "Ukrainian", Script::Cyrillic),
    native("ur", "Urdu", Script::Arabic),
    native("vi", "Vietnamese", Script::Latin),
    native("zh", "Chinese", Script::Han),
];

/// The registry entry for `tag`, ignoring surrounding whitespace and case.
pub fn language(tag: &str) -> Option<&'static Language> {
    let tag = tag.trim();
    LANGUAGES
        .iter()
        .find(|language| language.tag.eq_ignore_ascii_case(tag))
}

/// The language code models receive for `tag`; tags outside the registry pass through.
pub fn model_language(tag: &str) -> &str {
    language(tag).map_or(tag, |language| language.base)
}

/// The dictation languages as models understand them: variants become their base language,
/// without duplicates, order kept.
pub fn model_languages(tags: &[String]) -> Vec<String> {
    let mut codes: Vec<String> = Vec::with_capacity(tags.len());
    for tag in tags {
        let code = model_language(tag);
        if !codes.iter().any(|known| known == code) {
            codes.push(code.to_owned());
        }
    }
    codes
}

/// One registry entry as the language picker shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInfo {
    pub tag: String,
    pub name: String,
    /// The script, only when another entry shares this base language, so the picker can tell
    /// "Hindi · Devanagari" from "Hinglish · Roman script".
    pub script: Option<String>,
    pub primary: bool,
    /// May be the fallback language. Recognition is pinned to the fallback, and models only
    /// know base languages, so variants can't be.
    pub fallback: bool,
}

pub fn language_infos() -> Vec<LanguageInfo> {
    LANGUAGES
        .iter()
        .map(|language| {
            let shares_base = LANGUAGES
                .iter()
                .any(|other| other.tag != language.tag && other.base == language.base);
            LanguageInfo {
                tag: language.tag.to_owned(),
                name: language.name.to_owned(),
                script: shares_base.then(|| language.script.user_name().to_owned()),
                primary: language.primary,
                fallback: !language.is_variant(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(list: &[&str]) -> Vec<String> {
        list.iter().map(|tag| (*tag).to_owned()).collect()
    }

    #[test]
    fn lookup_is_exact_trimmed_and_case_insensitive() {
        assert_eq!(language("hi-Latn").map(|l| l.name), Some("Hinglish"));
        assert_eq!(language(" HI-latn ").map(|l| l.tag), Some("hi-Latn"));
        assert_eq!(language("EN").map(|l| l.tag), Some("en"));
        assert_eq!(language("hi-Lat"), None);
        assert_eq!(language("hinglish"), None);
        assert_eq!(language(""), None);
    }

    #[test]
    fn tags_are_unique_and_variants_have_a_native_sibling() {
        for (index, entry) in LANGUAGES.iter().enumerate() {
            assert!(
                LANGUAGES[index + 1..]
                    .iter()
                    .all(|other| other.tag != entry.tag),
                "{} is listed twice",
                entry.tag
            );
            if entry.is_variant() {
                let base = language(entry.base);
                assert!(base.is_some_and(|base| base.script == entry.native && !base.is_variant()));
            } else {
                assert_eq!(entry.base, entry.tag);
            }
        }
    }

    #[test]
    fn models_hear_variants_as_their_base_once() {
        assert_eq!(
            model_languages(&tags(&["en", "hi", "hi-Latn"])),
            tags(&["en", "hi"])
        );
        assert_eq!(
            model_languages(&tags(&["hi-Latn", "en"])),
            tags(&["hi", "en"])
        );
        assert_eq!(model_languages(&tags(&["de", "fr"])), tags(&["de", "fr"]));
        assert_eq!(model_language("hi-Latn"), "hi");
        assert_eq!(model_language("en"), "en");
        assert_eq!(model_language("xx"), "xx");
    }

    #[test]
    fn scripts_know_their_letters() {
        assert!(Script::Devanagari.contains('क'));
        assert!(!Script::Devanagari.contains('k'));
        assert!(Script::Latin.contains('k'));
        assert!(Script::Latin.contains('é'));
        assert!(Script::Latin.contains('ł'));
        assert!(!Script::Latin.contains('1'));
        assert!(!Script::Latin.contains('×'));
        assert!(!Script::Latin.contains('क'));
        assert!(Script::Tamil.contains('த'));
        assert!(!Script::Tamil.contains('क'));
        assert!(!Script::Tamil.contains('t'));
    }

    #[test]
    fn only_languages_sharing_a_base_carry_a_script_label() {
        let infos = language_infos();
        let info = |tag: &str| infos.iter().find(|info| info.tag == tag).cloned();
        let hindi = info("hi");
        let hinglish = info("hi-Latn");
        let english = info("en");
        assert_eq!(
            hindi.as_ref().and_then(|i| i.script.as_deref()),
            Some("Devanagari")
        );
        assert_eq!(
            hinglish.as_ref().and_then(|i| i.script.as_deref()),
            Some("Roman script")
        );
        assert_eq!(english.as_ref().map(|i| i.script.is_none()), Some(true));
        assert_eq!(
            hinglish.as_ref().map(|i| (i.primary, i.fallback)),
            Some((true, false))
        );
        assert_eq!(hindi.map(|i| (i.primary, i.fallback)), Some((true, true)));
        assert_eq!(
            info("de").map(|i| (i.primary, i.fallback)),
            Some((false, true))
        );
    }
}
