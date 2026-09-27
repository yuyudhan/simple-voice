// FilePath: src-tauri/src/features/models/smart_select.rs
//! Smart Select: which voice model runs a dictation, and when a second model double-checks it.
//! The user's languages pick one local model; a saved Groq key puts Groq Whisper in front of
//! it. Only ready models run, at most one retry follows, and it runs when the first model
//! errored or its result falls below that model's confidence floor. Nothing here does I/O, so
//! the whole policy is unit-tested.

use sv_domain::models::{
    GROQ_WHISPER, PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH, WHISPER_TURBO,
};
use sv_domain::text_stats::word_count;
use sv_domain::{
    AppError, AppResult, RetryReason, SmartSelectPlan, SmartSelectRow, TranscriptQuality,
};

/// The local model Hindi and Hinglish dictations go to; the evaluation of the two Whisper
/// candidates decides which one this is.
pub(crate) const LOCAL_HINGLISH: &str = WHISPER_HINGLISH;
/// A Whisper result whose token-weighted log probability is below this is unsure.
pub(crate) const WHISPER_LOGPROB_FLOOR: f32 = -1.0;
/// A Whisper result compressing better than this repeats itself (a typical hallucination).
pub(crate) const WHISPER_COMPRESSION_CEILING: f32 = 2.4;
/// Above this no-speech probability a low log probability means silence, which another model
/// would not hear better.
pub(crate) const WHISPER_NO_SPEECH_FLOOR: f32 = 0.6;
/// Shorter clips skip the gate: a word or two scores low without being wrong.
const MIN_CHECKED_WORDS: usize = 3;

pub(crate) const NO_READY_MODEL: &str =
    "No voice model is ready. Download one in Settings → Transcription";

/// Languages Parakeet TDT v3 transcribes.
const PARAKEET_V3_LANGUAGES: &[&str] = &[
    "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv", "lt", "mt",
    "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
];

/// English names for the plan: the list the language picker offers
/// (`src/features/settings/transcription/languages.ts`) plus the rest of Parakeet v3's.
const LANGUAGE_NAMES: &[(&str, &str)] = &[
    ("en", "English"),
    ("hi", "Hindi"),
    ("af", "Afrikaans"),
    ("ar", "Arabic"),
    ("bg", "Bulgarian"),
    ("bn", "Bengali"),
    ("ca", "Catalan"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("de", "German"),
    ("el", "Greek"),
    ("es", "Spanish"),
    ("et", "Estonian"),
    ("fa", "Persian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("gu", "Gujarati"),
    ("he", "Hebrew"),
    ("hr", "Croatian"),
    ("hu", "Hungarian"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("kn", "Kannada"),
    ("ko", "Korean"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("ml", "Malayalam"),
    ("mr", "Marathi"),
    ("ms", "Malay"),
    ("mt", "Maltese"),
    ("ne", "Nepali"),
    ("nl", "Dutch"),
    ("no", "Norwegian"),
    ("pa", "Punjabi"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("sr", "Serbian"),
    ("sv", "Swedish"),
    ("sw", "Swahili"),
    ("ta", "Tamil"),
    ("te", "Telugu"),
    ("th", "Thai"),
    ("tl", "Tagalog"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("vi", "Vietnamese"),
    ("zh", "Chinese"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LanguageClass {
    /// Hindi is among the languages (Hinglish is Hindi and English mixed).
    Hindi,
    /// Only English.
    English,
    /// Every language is one Parakeet v3 knows.
    European,
    Other,
}

pub(crate) fn language_class(languages: &[String]) -> LanguageClass {
    if languages.iter().any(|code| is(code, "hi")) {
        LanguageClass::Hindi
    } else if languages.iter().all(|code| is(code, "en")) {
        LanguageClass::English
    } else if languages
        .iter()
        .all(|code| PARAKEET_V3_LANGUAGES.iter().any(|known| is(code, known)))
    {
        LanguageClass::European
    } else {
        LanguageClass::Other
    }
}

/// The one local model for the languages; also the model worth keeping in memory.
pub(crate) fn local_model(languages: &[String]) -> &'static str {
    match language_class(languages) {
        LanguageClass::Hindi => LOCAL_HINGLISH,
        LanguageClass::English => PARAKEET_TDT_V2,
        LanguageClass::European => PARAKEET_TDT_V3,
        LanguageClass::Other => WHISPER_TURBO,
    }
}

/// The models in the order they are tried: Groq Whisper first when a key is saved, then the
/// local model.
pub(crate) fn route(languages: &[String], groq: bool) -> Vec<&'static str> {
    let local = local_model(languages);
    if groq {
        vec![GROQ_WHISPER, local]
    } else {
        vec![local]
    }
}

/// A route narrowed to the models that are ready right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadyRoute {
    pub(crate) first: &'static str,
    /// The model that double-checks the first one, if another is ready.
    pub(crate) retry: Option<&'static str>,
}

pub(crate) fn ready_route(
    route: &[&'static str],
    is_ready: impl Fn(&str) -> bool,
) -> AppResult<ReadyRoute> {
    let mut ready = route.iter().copied().filter(|model| is_ready(model));
    let first = ready
        .next()
        .ok_or_else(|| AppError::invalid(NO_READY_MODEL))?;
    Ok(ReadyRoute {
        first,
        retry: ready.next(),
    })
}

/// Whether a successful first result should be double-checked by the local model. The first
/// model of a two-model route is always Groq Whisper, so only Whisper's numbers are read.
pub(crate) fn gate(
    text: &str,
    quality: &TranscriptQuality,
    languages: &[String],
    smart_retry: bool,
) -> Option<RetryReason> {
    if !smart_retry || word_count(text) < MIN_CHECKED_WORDS {
        return None;
    }
    if let Some(logprob) = quality.avg_logprob {
        let doubtful = logprob < WHISPER_LOGPROB_FLOOR
            || quality
                .compression_ratio
                .is_some_and(|ratio| ratio > WHISPER_COMPRESSION_CEILING);
        let silence = quality
            .no_speech_prob
            .is_some_and(|prob| prob > WHISPER_NO_SPEECH_FLOOR)
            && logprob < WHISPER_LOGPROB_FLOOR;
        if doubtful && !silence {
            return Some(RetryReason::LowConfidence);
        }
    }
    // Groq often writes Hinglish half in Devanagari with full confidence ("कल का stand up");
    // Hinglish is written romanised, which the local Hinglish model does. Pure Devanagari is
    // Hindi and pure Latin is English or romanised Hinglish; both pass.
    let hindi = languages.iter().any(|code| is(code, "hi"));
    (hindi && is_mixed_script(text)).then_some(RetryReason::MixedScript)
}

/// Devanagari letters and ASCII Latin letters in the same text.
fn is_mixed_script(text: &str) -> bool {
    let devanagari = text.chars().any(|c| ('\u{0900}'..='\u{097F}').contains(&c));
    devanagari && text.chars().any(|c| c.is_ascii_alphabetic())
}

/// What Smart Select runs for `languages`, for its card in Settings → Transcription.
pub(crate) fn plan(languages: &[String], groq: bool) -> SmartSelectPlan {
    let mut names = Vec::with_capacity(languages.len() + 1);
    for code in languages {
        names.push(language_name(code));
        // Hindi speech is written as Hindi or romanised Hinglish; the local model does both.
        if is(code, "hi") {
            names.push("Hinglish".to_owned());
        }
    }
    SmartSelectPlan {
        groq,
        rows: vec![SmartSelectRow {
            purpose: format!("For {}", join_names(&names)),
            model: local_model(languages).to_owned(),
        }],
    }
}

fn language_name(code: &str) -> String {
    LANGUAGE_NAMES
        .iter()
        .find(|(known, _)| is(code, known))
        .map_or_else(
            || code.trim().to_uppercase(),
            |(_, name)| (*name).to_owned(),
        )
}

/// "German", "German and French", "English, German and French".
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn is(code: &str, expected: &str) -> bool {
    code.trim().eq_ignore_ascii_case(expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn langs(codes: &[&str]) -> Vec<String> {
        codes.iter().map(|code| (*code).to_owned()).collect()
    }

    fn whisper(logprob: f32, ratio: f32, no_speech: f32) -> TranscriptQuality {
        TranscriptQuality {
            avg_logprob: Some(logprob),
            compression_ratio: Some(ratio),
            no_speech_prob: Some(no_speech),
            confidence: None,
        }
    }

    const HINDI: &[&str] = &["en", "hi"];

    const SENTENCE: &str = "move the standup to ten";

    #[test]
    fn languages_fall_into_classes() {
        assert_eq!(language_class(&langs(&["en", "hi"])), LanguageClass::Hindi);
        assert_eq!(language_class(&langs(&["hi"])), LanguageClass::Hindi);
        assert_eq!(language_class(&langs(&["en"])), LanguageClass::English);
        assert_eq!(
            language_class(&langs(&["en", "de"])),
            LanguageClass::European
        );
        assert_eq!(
            language_class(&langs(&["uk", "ru"])),
            LanguageClass::European
        );
        assert_eq!(language_class(&langs(&["en", "ja"])), LanguageClass::Other);
        // Hindi wins over everything else in the list.
        assert_eq!(language_class(&langs(&["ja", "hi"])), LanguageClass::Hindi);
    }

    #[test]
    fn every_class_has_one_local_model_behind_an_optional_groq() {
        let hindi = langs(&["en", "hi"]);
        assert_eq!(route(&hindi, false), [LOCAL_HINGLISH]);
        assert_eq!(route(&hindi, true), [GROQ_WHISPER, LOCAL_HINGLISH]);
        assert_eq!(route(&langs(&["en"]), false), [PARAKEET_TDT_V2]);
        assert_eq!(
            route(&langs(&["fr", "de"]), true),
            [GROQ_WHISPER, PARAKEET_TDT_V3]
        );
        assert_eq!(route(&langs(&["ja"]), false), [WHISPER_TURBO]);
    }

    #[test]
    fn only_ready_models_run() {
        let with_groq = route(&langs(&["en", "hi"]), true);
        assert_eq!(
            ready_route(&with_groq, |_| true).unwrap(),
            ReadyRoute {
                first: GROQ_WHISPER,
                retry: Some(LOCAL_HINGLISH)
            }
        );
        // Hinglish Whisper not downloaded: Groq runs alone.
        assert_eq!(
            ready_route(&with_groq, |model| model != LOCAL_HINGLISH).unwrap(),
            ReadyRoute {
                first: GROQ_WHISPER,
                retry: None
            }
        );
        let local = route(&langs(&["en"]), false);
        assert_eq!(
            ready_route(&local, |_| true).unwrap(),
            ReadyRoute {
                first: PARAKEET_TDT_V2,
                retry: None
            }
        );
    }

    #[test]
    fn nothing_ready_is_an_error_naming_where_to_download() {
        let hindi = route(&langs(&["en", "hi"]), false);
        let error = ready_route(&hindi, |_| false).unwrap_err();
        assert_eq!(error, AppError::invalid(NO_READY_MODEL));
        assert!(error.to_string().contains("Settings → Transcription"));
    }

    fn gated(text: &str, quality: &TranscriptQuality, languages: &[&str]) -> Option<RetryReason> {
        gate(text, quality, &langs(languages), true)
    }

    #[test]
    fn whisper_gate_flags_low_logprob_and_repetition() {
        assert_eq!(gated(SENTENCE, &whisper(-0.3, 1.2, 0.01), HINDI), None);
        assert_eq!(
            gated(SENTENCE, &whisper(-1.2, 1.2, 0.01), HINDI),
            Some(RetryReason::LowConfidence)
        );
        assert_eq!(
            gated(SENTENCE, &whisper(-0.3, 2.6, 0.01), &["en"]),
            Some(RetryReason::LowConfidence)
        );
        // Exactly at the floors is still fine.
        let edge = whisper(WHISPER_LOGPROB_FLOOR, WHISPER_COMPRESSION_CEILING, 0.0);
        assert_eq!(gated(SENTENCE, &edge, HINDI), None);
    }

    #[test]
    fn whisper_gate_exempts_silence() {
        // Likely silence: another model would not hear more.
        assert_eq!(gated(SENTENCE, &whisper(-1.4, 1.0, 0.8), HINDI), None);
        // A high no-speech probability alone does not hide repetition.
        assert_eq!(
            gated(SENTENCE, &whisper(-0.5, 2.8, 0.8), HINDI),
            Some(RetryReason::LowConfidence)
        );
    }

    #[test]
    fn confident_hinglish_half_in_devanagari_is_mixed_script() {
        let confident = whisper(-0.1, 1.1, 0.01);
        let mixed = "कल का stand up 11 बजे shift कर दो.";
        assert_eq!(
            gated(mixed, &confident, HINDI),
            Some(RetryReason::MixedScript)
        );
        assert_eq!(
            gated(mixed, &confident, &["hi"]),
            Some(RetryReason::MixedScript)
        );
        // Without quality numbers the script check still runs.
        assert_eq!(
            gated(mixed, &TranscriptQuality::default(), HINDI),
            Some(RetryReason::MixedScript)
        );
    }

    #[test]
    fn one_script_or_no_hindi_is_not_mixed_script() {
        let confident = whisper(-0.1, 1.1, 0.01);
        let hindi = "कल की मीटिंग ग्यारह बजे है";
        let romanised = "kal ka stand up 11 baje shift kar do";
        assert_eq!(gated(hindi, &confident, HINDI), None);
        assert_eq!(gated(romanised, &confident, HINDI), None);
        assert_eq!(gated(SENTENCE, &confident, HINDI), None);
        // Hindi not allowed: mixed script is not Smart Select's concern.
        let mixed = "कल का stand up 11 बजे shift कर दो.";
        assert_eq!(gated(mixed, &confident, &["en", "mr"]), None);
    }

    #[test]
    fn short_clips_unreported_quality_and_retry_off_are_never_gated() {
        let unsure = whisper(-3.0, 4.0, 0.0);
        assert_eq!(gated("ship it", &unsure, HINDI), None);
        assert_eq!(gated("कल stand", &unsure, HINDI), None);
        assert_eq!(
            gated("one two three", &unsure, HINDI),
            Some(RetryReason::LowConfidence)
        );
        assert_eq!(gated(SENTENCE, &TranscriptQuality::default(), HINDI), None);
        let hindi = langs(HINDI);
        assert_eq!(gate(SENTENCE, &unsure, &hindi, false), None);
        assert_eq!(
            gate(
                "कल का stand up",
                &TranscriptQuality::default(),
                &hindi,
                false
            ),
            None
        );
    }

    #[test]
    fn plan_is_one_row_naming_the_languages() {
        let row = |purpose: &str, model: &str| SmartSelectRow {
            purpose: purpose.to_owned(),
            model: model.to_owned(),
        };
        let hindi = plan(&langs(&["en", "hi"]), true);
        assert!(hindi.groq);
        assert_eq!(
            hindi.rows,
            [row("For English, Hindi and Hinglish", LOCAL_HINGLISH)]
        );
        let english = plan(&langs(&["en"]), false);
        assert!(!english.groq);
        assert_eq!(english.rows, [row("For English", PARAKEET_TDT_V2)]);
        assert_eq!(
            plan(&langs(&["de", "fr"]), false).rows,
            [row("For German and French", PARAKEET_TDT_V3)]
        );
        assert_eq!(
            plan(&langs(&["et", "sl"]), false).rows,
            [row("For Estonian and Slovenian", PARAKEET_TDT_V3)]
        );
        assert_eq!(
            plan(&langs(&["ja", "xx", "ko"]), false).rows,
            [row("For Japanese, XX and Korean", WHISPER_TURBO)]
        );
    }
}
