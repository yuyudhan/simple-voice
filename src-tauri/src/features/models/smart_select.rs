// FilePath: src-tauri/src/features/models/smart_select.rs
//! Smart Select: which voice model runs a dictation, and when a second model double-checks it.
//! The user's languages pick one local model; a saved Groq key puts Groq Whisper in front of
//! it. Only ready models run, at most one retry follows, and it runs when the first model
//! errored or its result falls below that model's confidence floor. Routing reads the language
//! registry and each catalog model's coverage, so nothing here names a language. Nothing here
//! does I/O, so the whole policy is unit-tested.

use sv_domain::languages::{language, Language, Script};
use sv_domain::models::{model_name, GROQ_WHISPER, WHISPER_TURBO};
use sv_domain::text_stats::word_count;
use sv_domain::{AppError, AppResult, RetryReason, SmartSelectPlan, TranscriptQuality};

use super::catalog::{CatalogEntry, Coverage, CATALOG};

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

/// The registry entries of the selected tags; tags outside the registry (which settings
/// validation rejects) are skipped.
fn selected(languages: &[String]) -> Vec<&'static Language> {
    languages.iter().filter_map(|tag| language(tag)).collect()
}

/// The candidate covering the most selected languages; ties go to the more accurate model,
/// then the smaller download.
fn local_entry(selected: &[&Language]) -> Option<(&'static CatalogEntry, Coverage)> {
    let covered = |coverage: Coverage| {
        selected
            .iter()
            .filter(|language| coverage.covers(language))
            .count()
    };
    CATALOG
        .iter()
        .filter_map(|entry| entry.covers.map(|coverage| (entry, coverage)))
        .max_by(|(a, a_covers), (b, b_covers)| {
            covered(*a_covers)
                .cmp(&covered(*b_covers))
                .then(a.accuracy.cmp(&b.accuracy))
                .then(size(b).cmp(&size(a)))
        })
}

fn size(entry: &CatalogEntry) -> u32 {
    entry.size_mb.unwrap_or(u32::MAX)
}

/// The one local model for the languages; also the model worth keeping in memory.
pub(crate) fn local_model(languages: &[String]) -> &'static str {
    // The catalog always has candidates; Whisper Turbo, the widest of them, stands in so an
    // empty candidate list still names a model.
    local_entry(&selected(languages)).map_or(WHISPER_TURBO, |(entry, _)| entry.id)
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
    // Groq writes a romanised variant half in the base language's own script with full
    // confidence ("कल का stand up"); the local model writes it the chosen way. Text in one
    // script passes: it is either the base language or the variant.
    let mixed = selected(languages).iter().any(|variant| {
        variant.is_variant()
            && has_letters(text, variant.native)
            && has_letters(text, variant.script)
    });
    mixed.then_some(RetryReason::MixedScript)
}

fn has_letters(text: &str, script: Script) -> bool {
    text.chars().any(|c| script.contains(c))
}

/// What Smart Select runs for `languages`, for its card in Settings → Transcription.
pub(crate) fn plan(languages: &[String], groq: bool) -> SmartSelectPlan {
    let selected = selected(languages);
    let local = local_entry(&selected);
    SmartSelectPlan {
        groq,
        local: local
            .map_or(WHISPER_TURBO, |(entry, _)| entry.id)
            .to_owned(),
        notice: local.and_then(|(entry, coverage)| notice(&selected, entry, coverage)),
    }
}

/// Why some selected languages won't come out as chosen offline, or `None` when all will. A
/// language whose sibling (same spoken language, other script) is covered is written like the
/// sibling, which the user can change by removing the sibling.
fn notice(selected: &[&Language], entry: &CatalogEntry, coverage: Coverage) -> Option<String> {
    let mut sentences = Vec::new();
    let mut unwritten = Vec::new();
    for missing in selected
        .iter()
        .filter(|language| !coverage.covers(language))
    {
        let sibling = selected.iter().find(|other| {
            other.tag != missing.tag && other.base == missing.base && coverage.covers(other)
        });
        match sibling {
            Some(sibling) => sentences.push(format!(
                "Offline, {} is written in {}. Remove {} to get {} offline.",
                missing.name,
                sibling.script.user_name(),
                sibling.name,
                missing.script.user_name()
            )),
            None => unwritten.push(missing.name),
        }
    }
    if !unwritten.is_empty() {
        sentences.push(format!(
            "{} can't transcribe {} offline.",
            model_name(entry.id).unwrap_or(entry.id),
            join_names(&unwritten)
        ));
    }
    (!sentences.is_empty()).then(|| sentences.join(" "))
}

/// "German", "German and French", "English, German and French".
fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [only] => (*only).to_owned(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use sv_domain::models::{PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH};

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

    /// The default languages.
    const DEFAULT_SET: &[&str] = &["en", "hi", "hi-Latn"];

    const SENTENCE: &str = "move the standup to ten";

    #[test]
    fn languages_route_to_the_model_covering_most_of_them() {
        let cases: &[(&[&str], &str)] = &[
            (&["en"], PARAKEET_TDT_V2),
            (&["de", "fr"], PARAKEET_TDT_V3),
            (&["en", "de"], PARAKEET_TDT_V3),
            (&["uk", "ru"], PARAKEET_TDT_V3),
            (&["et", "sl"], PARAKEET_TDT_V3),
            (&["en", "hi"], WHISPER_TURBO),
            (&["hi"], WHISPER_TURBO),
            (&["en", "hi-Latn"], WHISPER_HINGLISH),
            (&["hi-Latn"], WHISPER_HINGLISH),
            (DEFAULT_SET, WHISPER_HINGLISH),
            (&["ja"], WHISPER_TURBO),
            (&["en", "ja"], WHISPER_TURBO),
            (&["en", "hi", "hi-Latn", "ja"], WHISPER_TURBO),
        ];
        for (languages, expected) in cases {
            assert_eq!(local_model(&langs(languages)), *expected, "{languages:?}");
        }
    }

    #[test]
    fn groq_runs_in_front_of_the_local_model() {
        assert_eq!(route(&langs(DEFAULT_SET), false), [WHISPER_HINGLISH]);
        assert_eq!(
            route(&langs(DEFAULT_SET), true),
            [GROQ_WHISPER, WHISPER_HINGLISH]
        );
        assert_eq!(
            route(&langs(&["fr", "de"]), true),
            [GROQ_WHISPER, PARAKEET_TDT_V3]
        );
    }

    #[test]
    fn only_ready_models_run() {
        let with_groq = route(&langs(DEFAULT_SET), true);
        assert_eq!(
            ready_route(&with_groq, |_| true).unwrap(),
            ReadyRoute {
                first: GROQ_WHISPER,
                retry: Some(WHISPER_HINGLISH)
            }
        );
        // The local model not downloaded: Groq runs alone.
        assert_eq!(
            ready_route(&with_groq, |model| model != WHISPER_HINGLISH).unwrap(),
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

    #[test]
    fn plan_names_the_local_model_and_what_it_cannot_write() {
        let cases: &[(&[&str], &str, Option<&str>)] = &[
            (&["en"], PARAKEET_TDT_V2, None),
            (&["en", "hi"], WHISPER_TURBO, None),
            (&["en", "hi-Latn"], WHISPER_HINGLISH, None),
            (
                DEFAULT_SET,
                WHISPER_HINGLISH,
                Some(
                    "Offline, Hindi is written in Roman script. \
                     Remove Hinglish to get Devanagari offline.",
                ),
            ),
            (
                &["en", "hi", "hi-Latn", "ja"],
                WHISPER_TURBO,
                Some(
                    "Offline, Hinglish is written in Devanagari. \
                     Remove Hindi to get Roman script offline.",
                ),
            ),
            (
                &["en", "hi-Latn", "ja"],
                WHISPER_HINGLISH,
                Some("Hinglish Whisper can't transcribe Japanese offline."),
            ),
            (
                &["en", "hi-Latn", "ja", "ko"],
                WHISPER_TURBO,
                Some("Whisper Turbo can't transcribe Hinglish offline."),
            ),
        ];
        for (languages, local, notice) in cases {
            let plan = plan(&langs(languages), true);
            assert!(plan.groq);
            assert_eq!(plan.local, *local, "{languages:?}");
            assert_eq!(plan.notice.as_deref(), *notice, "{languages:?}");
        }
        assert!(!plan(&langs(&["en"]), false).groq);
    }

    #[test]
    fn names_join_like_a_sentence() {
        assert_eq!(join_names(&["German"]), "German");
        assert_eq!(join_names(&["German", "French"]), "German and French");
        assert_eq!(
            join_names(&["English", "German", "French"]),
            "English, German and French"
        );
    }

    fn gated(text: &str, quality: &TranscriptQuality, languages: &[&str]) -> Option<RetryReason> {
        gate(text, quality, &langs(languages), true)
    }

    #[test]
    fn whisper_gate_flags_low_logprob_and_repetition() {
        assert_eq!(
            gated(SENTENCE, &whisper(-0.3, 1.2, 0.01), DEFAULT_SET),
            None
        );
        assert_eq!(
            gated(SENTENCE, &whisper(-1.2, 1.2, 0.01), DEFAULT_SET),
            Some(RetryReason::LowConfidence)
        );
        assert_eq!(
            gated(SENTENCE, &whisper(-0.3, 2.6, 0.01), &["en"]),
            Some(RetryReason::LowConfidence)
        );
        // Exactly at the floors is still fine.
        let edge = whisper(WHISPER_LOGPROB_FLOOR, WHISPER_COMPRESSION_CEILING, 0.0);
        assert_eq!(gated(SENTENCE, &edge, DEFAULT_SET), None);
    }

    #[test]
    fn whisper_gate_exempts_silence() {
        // Likely silence: another model would not hear more.
        assert_eq!(gated(SENTENCE, &whisper(-1.4, 1.0, 0.8), DEFAULT_SET), None);
        // A high no-speech probability alone does not hide repetition.
        assert_eq!(
            gated(SENTENCE, &whisper(-0.5, 2.8, 0.8), DEFAULT_SET),
            Some(RetryReason::LowConfidence)
        );
    }

    #[test]
    fn mixed_script_needs_a_selected_variant_and_both_its_scripts() {
        let confident = whisper(-0.1, 1.1, 0.01);
        let mixed = "कल का stand up 11 बजे shift कर दो.";
        let hindi = "कल की मीटिंग ग्यारह बजे है";
        let romanised = "kal ka stand up 11 baje shift kar do";
        let cases: &[(&str, &[&str], Option<RetryReason>)] = &[
            (mixed, DEFAULT_SET, Some(RetryReason::MixedScript)),
            (mixed, &["hi-Latn"], Some(RetryReason::MixedScript)),
            (hindi, DEFAULT_SET, None),
            (romanised, DEFAULT_SET, None),
            (SENTENCE, DEFAULT_SET, None),
            // Without a variant, Devanagari with English words in it is fine Hindi or Marathi.
            (mixed, &["en", "hi"], None),
            (mixed, &["en", "mr"], None),
            (mixed, &["en"], None),
        ];
        for (text, languages, expected) in cases {
            assert_eq!(
                gated(text, &confident, languages),
                *expected,
                "{text} {languages:?}"
            );
        }
        // Without quality numbers the script check still runs.
        assert_eq!(
            gated(mixed, &TranscriptQuality::default(), DEFAULT_SET),
            Some(RetryReason::MixedScript)
        );
    }

    #[test]
    fn short_clips_unreported_quality_and_retry_off_are_never_gated() {
        let unsure = whisper(-3.0, 4.0, 0.0);
        assert_eq!(gated("ship it", &unsure, DEFAULT_SET), None);
        assert_eq!(gated("कल stand", &unsure, DEFAULT_SET), None);
        assert_eq!(
            gated("one two three", &unsure, DEFAULT_SET),
            Some(RetryReason::LowConfidence)
        );
        assert_eq!(
            gated(SENTENCE, &TranscriptQuality::default(), DEFAULT_SET),
            None
        );
        let defaults = langs(DEFAULT_SET);
        assert_eq!(gate(SENTENCE, &unsure, &defaults, false), None);
        assert_eq!(
            gate(
                "कल का stand up",
                &TranscriptQuality::default(),
                &defaults,
                false
            ),
            None
        );
    }
}
