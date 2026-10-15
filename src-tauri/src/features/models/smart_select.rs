// FilePath: src-tauri/src/features/models/smart_select.rs
//! Smart Select: which on-device voice model runs a dictation, and when a second one
//! double-checks it. The user's languages rank the local models by how many of them each writes
//! as chosen; the best ready one runs. At most one retry follows: on the next ready model that
//! writes every selected language the first one does, when the first model errored or its result
//! fell below that model's confidence floor; or on another ready model that writes the chosen
//! script, when the result was not written in it. Routing reads the language registry and each
//! catalog model's coverage, so nothing here names a language. Nothing here does I/O, so the
//! whole policy is unit-tested.

use sv_domain::languages::{language, Language, Script};
use sv_domain::models::{model_name, WHISPER_TURBO};
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

/// Every candidate that writes at least one selected language, best first: most selected
/// languages covered, then the more accurate model, then the smaller download.
fn ranked(selected: &[&Language]) -> Vec<(&'static CatalogEntry, Coverage)> {
    let covered = |coverage: Coverage| {
        selected
            .iter()
            .filter(|language| coverage.covers(language))
            .count()
    };
    let mut candidates: Vec<(&'static CatalogEntry, Coverage)> = CATALOG
        .iter()
        .filter_map(|entry| entry.covers.map(|coverage| (entry, coverage)))
        .filter(|(_, coverage)| covered(*coverage) > 0)
        .collect();
    candidates.sort_by(|(a, a_covers), (b, b_covers)| {
        covered(*b_covers)
            .cmp(&covered(*a_covers))
            .then(b.accuracy.cmp(&a.accuracy))
            .then(size(a).cmp(&size(b)))
    });
    candidates
}

/// The candidate covering the most selected languages.
fn local_entry(selected: &[&Language]) -> Option<(&'static CatalogEntry, Coverage)> {
    ranked(selected).into_iter().next()
}

fn size(entry: &CatalogEntry) -> u32 {
    entry.size_mb.unwrap_or(u32::MAX)
}

/// The best local model for the languages; also the model worth keeping in memory.
pub(crate) fn local_model(languages: &[String]) -> &'static str {
    // The catalog always has candidates; Whisper Turbo, the widest of them, stands in so a
    // selection no model covers still names a model.
    local_entry(&selected(languages)).map_or(WHISPER_TURBO, |(entry, _)| entry.id)
}

/// The models worth trying for the languages, best first.
pub(crate) fn route(languages: &[String]) -> Vec<&'static str> {
    ranked(&selected(languages))
        .into_iter()
        .map(|(entry, _)| entry.id)
        .collect()
}

/// The models a dictation runs right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReadyRoute {
    pub(crate) first: &'static str,
    /// The model that double-checks the first one, if a ready one writes every selected
    /// language the first one writes.
    pub(crate) retry: Option<&'static str>,
}

/// The best ready model, and the next ready one that covers at least the selected languages
/// the first covers (a retry must never write a chosen language worse).
pub(crate) fn ready_route(
    languages: &[String],
    is_ready: impl Fn(&str) -> bool,
) -> AppResult<ReadyRoute> {
    let selected = selected(languages);
    let mut ready = ranked(&selected)
        .into_iter()
        .filter(|(entry, _)| is_ready(entry.id));
    let (first, first_covers) = ready
        .next()
        .ok_or_else(|| AppError::invalid(NO_READY_MODEL))?;
    let retry = ready.find(|(_, coverage)| {
        selected
            .iter()
            .all(|language| !first_covers.covers(language) || coverage.covers(language))
    });
    Ok(ReadyRoute {
        first: first.id,
        retry: retry.map(|(entry, _)| entry.id),
    })
}

/// Whether a successful first result should be double-checked by another model. Only Whisper
/// reports the confidence numbers read here; every result's script is checked.
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
    script_violation(text, &selected(languages)).map(|(reason, _)| reason)
}

/// Why `text` is not written the way the selected languages are, and the languages it got
/// wrong. English words in Roman script inside Hindi text are how Hindi is written, so Roman
/// letters only count against Hindi when the text has no Devanagari at all.
fn script_violation<'a>(
    text: &str,
    selected: &[&'a Language],
) -> Option<(RetryReason, Vec<&'a Language>)> {
    // Whisper can write a romanised variant half in the base language's own script with full
    // confidence ("कल का stand up"). Text in one script is either the base language or the
    // variant, which the checks below tell apart.
    let mixed: Vec<&Language> = selected
        .iter()
        .copied()
        .filter(|variant| {
            variant.is_variant()
                && has_letters(text, variant.native)
                && has_letters(text, variant.script)
        })
        .collect();
    if !mixed.is_empty() {
        return Some((RetryReason::MixedScript, mixed));
    }
    // A variant's native script, which no selected language is written in (Hinglish chosen,
    // Devanagari heard back).
    let written = |script: Script| selected.iter().any(|language| language.script == script);
    let native: Vec<&Language> = selected
        .iter()
        .copied()
        .filter(|variant| {
            variant.is_variant() && !written(variant.native) && has_letters(text, variant.native)
        })
        .collect();
    if !native.is_empty() {
        return Some((RetryReason::WrongScript, native));
    }
    // Letters, none of them in a selected script (Hindi chosen, all Roman script heard back).
    let mut letters = text.chars().filter(|c| c.is_alphabetic()).peekable();
    let foreign = !selected.is_empty()
        && letters.peek().is_some()
        && letters.all(|c| !selected.iter().any(|language| language.script.contains(c)));
    foreign.then(|| (RetryReason::WrongScript, selected.to_vec()))
}

fn has_letters(text: &str, script: Script) -> bool {
    text.chars().any(|c| script.contains(c))
}

/// The models that could redo a result [`gate`] found in the wrong script, best first: models
/// other than `first` that write at least one language the result got wrong, those writing more
/// of them first, then more of the selected languages, then the more accurate model and the
/// smaller download. Unlike [`ReadyRoute::retry`] such a model may write another selected
/// language worse: the first result already missed the chosen script. The caller runs the first
/// one that is ready.
pub(crate) fn script_retry(text: &str, languages: &[String], first: &str) -> Vec<&'static str> {
    let selected = selected(languages);
    let Some((_, wrong)) = script_violation(text, &selected) else {
        return Vec::new();
    };
    let count = |coverage: Coverage, of: &[&Language]| {
        of.iter()
            .filter(|language| coverage.covers(language))
            .count()
    };
    let mut candidates: Vec<(&'static CatalogEntry, Coverage)> = CATALOG
        .iter()
        .filter(|entry| entry.id != first)
        .filter_map(|entry| entry.covers.map(|coverage| (entry, coverage)))
        .filter(|(_, coverage)| count(*coverage, &wrong) > 0)
        .collect();
    candidates.sort_by(|(a, a_covers), (b, b_covers)| {
        count(*b_covers, &wrong)
            .cmp(&count(*a_covers, &wrong))
            .then(count(*b_covers, &selected).cmp(&count(*a_covers, &selected)))
            .then(b.accuracy.cmp(&a.accuracy))
            .then(size(a).cmp(&size(b)))
    });
    candidates.into_iter().map(|(entry, _)| entry.id).collect()
}

/// What Smart Select runs for `languages` given what `is_ready`, for its card in Settings →
/// Transcription: the model a dictation would run now (the same choice as [`ready_route`]),
/// the best model for the languages to recommend downloading, and what the running model
/// can't write as chosen.
pub(crate) fn plan(languages: &[String], is_ready: impl Fn(&str) -> bool) -> SmartSelectPlan {
    let selected = selected(languages);
    let ranked = ranked(&selected);
    let recommended = ranked.first().copied();
    let running = ranked.iter().copied().find(|(entry, _)| is_ready(entry.id));
    let shown = running.or(recommended);
    SmartSelectPlan {
        local: shown
            .map_or(WHISPER_TURBO, |(entry, _)| entry.id)
            .to_owned(),
        recommended: recommended
            .map_or(WHISPER_TURBO, |(entry, _)| entry.id)
            .to_owned(),
        ready: running.is_some(),
        notice: shown.and_then(|(entry, coverage)| notice(&selected, entry, coverage)),
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
                "{} is written in {}. Remove {} to get {}.",
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
            "{} can't transcribe {}.",
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
    fn route_ranks_every_model_writing_a_selected_language() {
        assert_eq!(
            route(&langs(DEFAULT_SET)),
            [
                WHISPER_HINGLISH,
                WHISPER_TURBO,
                PARAKEET_TDT_V2,
                PARAKEET_TDT_V3
            ]
        );
        assert_eq!(
            route(&langs(&["fr", "de"])),
            [PARAKEET_TDT_V3, WHISPER_TURBO]
        );
        // Nothing writes Hinglish but the Hinglish model.
        assert_eq!(route(&langs(&["hi-Latn"])), [WHISPER_HINGLISH]);
    }

    #[test]
    fn only_ready_models_run_and_a_retry_never_covers_less() {
        let defaults = langs(DEFAULT_SET);
        // Hinglish first; Turbo would write Hinglish in Devanagari, so it is no retry for it.
        assert_eq!(
            ready_route(&defaults, |_| true).unwrap(),
            ReadyRoute {
                first: WHISPER_HINGLISH,
                retry: None
            }
        );
        // Only Parakeet downloaded: it still transcribes the English it covers.
        assert_eq!(
            ready_route(&defaults, |model| model == PARAKEET_TDT_V3).unwrap(),
            ReadyRoute {
                first: PARAKEET_TDT_V3,
                retry: None
            }
        );
        let english = langs(&["en"]);
        assert_eq!(
            ready_route(&english, |_| true).unwrap(),
            ReadyRoute {
                first: PARAKEET_TDT_V2,
                retry: Some(PARAKEET_TDT_V3)
            }
        );
        let european = langs(&["de", "fr"]);
        assert_eq!(
            ready_route(&european, |_| true).unwrap(),
            ReadyRoute {
                first: PARAKEET_TDT_V3,
                retry: Some(WHISPER_TURBO)
            }
        );
    }

    #[test]
    fn nothing_ready_is_an_error_naming_where_to_download() {
        let error = ready_route(&langs(&["en", "hi"]), |_| false).unwrap_err();
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
                Some("Hindi is written in Roman script. Remove Hinglish to get Devanagari."),
            ),
            (
                &["en", "hi", "hi-Latn", "ja"],
                WHISPER_TURBO,
                Some("Hinglish is written in Devanagari. Remove Hindi to get Roman script."),
            ),
            (
                &["en", "hi-Latn", "ja"],
                WHISPER_HINGLISH,
                Some("Hinglish Whisper can't transcribe Japanese."),
            ),
            (
                &["en", "hi-Latn", "ja", "ko"],
                WHISPER_TURBO,
                Some("Whisper Turbo can't transcribe Hinglish."),
            ),
        ];
        for (languages, local, notice) in cases {
            let plan = plan(&langs(languages), |_| true);
            assert_eq!(plan.local, *local, "{languages:?}");
            assert_eq!(plan.recommended, *local, "{languages:?}");
            assert!(plan.ready);
            assert_eq!(plan.notice.as_deref(), *notice, "{languages:?}");
        }
    }

    #[test]
    fn plan_shows_the_model_that_runs_now_and_still_recommends_the_best_one() {
        let defaults = langs(DEFAULT_SET);
        // Hinglish Whisper missing, Parakeet v3 and Turbo downloaded: Turbo runs, as the
        // dictation route would choose, and its own limits are what the notice explains.
        let qa = |model: &str| model == PARAKEET_TDT_V3 || model == WHISPER_TURBO;
        let shown = plan(&defaults, qa);
        assert_eq!(shown.local, WHISPER_TURBO);
        assert_eq!(
            Some(shown.local.as_str()),
            ready_route(&defaults, qa).ok().map(|route| route.first)
        );
        assert_eq!(shown.recommended, WHISPER_HINGLISH);
        assert!(shown.ready);
        assert_eq!(
            shown.notice.as_deref(),
            Some("Hinglish is written in Devanagari. Remove Hindi to get Roman script.")
        );
        // Nothing downloaded: the recommendation is shown, marked not ready.
        let none = plan(&defaults, |_| false);
        assert_eq!(none.local, WHISPER_HINGLISH);
        assert!(!none.ready);
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
    fn wrong_script_flags_text_outside_the_chosen_script_both_ways() {
        let confident = whisper(-0.1, 1.1, 0.01);
        let hindi = "कल की मीटिंग ग्यारह बजे है";
        let hindi_with_english = "कल की meeting ग्यारह बजे है";
        let romanised = "kal ki meeting gyarah baje hai";
        let cases: &[(&str, &[&str], Option<RetryReason>)] = &[
            // Hinglish chosen: any Devanagari is wrong unless Hindi is chosen too.
            (hindi, &["hi-Latn"], Some(RetryReason::WrongScript)),
            (hindi, &["en", "hi-Latn"], Some(RetryReason::WrongScript)),
            (romanised, &["hi-Latn"], None),
            // Hindi chosen: Roman-only text is wrong; English words inside Devanagari are not.
            (romanised, &["hi"], Some(RetryReason::WrongScript)),
            (hindi_with_english, &["hi"], None),
            (hindi, &["hi"], None),
            // With English chosen too, Roman text may be English.
            (romanised, &["en", "hi"], None),
            (hindi, DEFAULT_SET, None),
            // Only English chosen: Devanagari without a Roman letter is in no chosen script.
            (hindi, &["en"], Some(RetryReason::WrongScript)),
        ];
        for (text, languages, expected) in cases {
            assert_eq!(
                gated(text, &confident, languages),
                *expected,
                "{text} {languages:?}"
            );
        }
    }

    #[test]
    fn script_retries_go_to_another_model_writing_what_was_wrong() {
        let hindi = "कल की मीटिंग ग्यारह बजे है";
        let romanised = "kal ki meeting gyarah baje hai";
        let mixed = "कल का stand up 11 बजे shift कर दो.";
        let cases: &[(&str, &[&str], &str, &[&str])] = &[
            (
                hindi,
                &["en", "hi-Latn"],
                WHISPER_TURBO,
                &[WHISPER_HINGLISH],
            ),
            (mixed, DEFAULT_SET, WHISPER_TURBO, &[WHISPER_HINGLISH]),
            (romanised, &["hi"], WHISPER_HINGLISH, &[WHISPER_TURBO]),
            // The first model is never its own retry.
            (romanised, &["hi"], WHISPER_TURBO, &[]),
            (mixed, DEFAULT_SET, WHISPER_HINGLISH, &[]),
            // Several candidates: most accurate first, then the smaller download.
            (
                hindi,
                &["en"],
                WHISPER_TURBO,
                &[PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_HINGLISH],
            ),
            // Written as chosen: nothing to retry.
            (romanised, &["hi-Latn"], PARAKEET_TDT_V3, &[]),
        ];
        for (text, languages, first, expected) in cases {
            assert_eq!(
                script_retry(text, &langs(languages), first),
                *expected,
                "{text} {languages:?} {first}"
            );
        }
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
