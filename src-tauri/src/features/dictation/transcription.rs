// FilePath: src-tauri/src/features/dictation/transcription.rs
//! Speech to text for dictations, their retries and edit instructions: Groq Whisper or the
//! engine helper's local models (or Smart Select choosing among them), the personal dictionary
//! that primes them, and the session WAV files the local models read.

use std::path::PathBuf;
use std::time::Instant;

use sv_domain::models::{GROQ_WHISPER, SMART_SELECT, WHISPER_HINGLISH, WHISPER_TURBO};
use sv_domain::{AppError, AppResult, RetryReason, Settings, TranscriptQuality};
use sv_engine::TranscribeOptions;
use sv_text::Vocabulary;

use super::elapsed_ms;
use crate::features::history::remove_audio;
use crate::features::models::smart_select::{self, ReadyRoute};
use crate::features::models::{engine_language, is_ready};
use crate::state::{now_ms, AppState};

const NO_GROQ_KEY: &str = "Add your Groq API key in Settings → Transcription";

pub(super) struct Heard {
    pub(super) text: String,
    pub(super) language: Option<String>,
    /// Time the model that produced `text` took, excluding recording and file writes.
    pub(super) ms: i64,
    /// The model that produced `text`.
    pub(super) model: String,
    /// The model Smart Select tried first when it ran a second one; `None` otherwise.
    pub(super) first_model: Option<String>,
    pub(super) retry_reason: Option<RetryReason>,
}

/// A transcription that produced no text: why, and which models it tried (for history).
pub(super) struct Unheard {
    pub(super) error: AppError,
    /// The last model tried; the configured choice when none ran.
    pub(super) model: String,
    pub(super) first_model: Option<String>,
    pub(super) retry_reason: Option<RetryReason>,
}

impl Unheard {
    /// Failed before any model ran.
    fn untried(settings: &Settings, error: AppError) -> Self {
        Self::from_model(&settings.transcription_model, error)
    }

    fn from_model(model: &str, error: AppError) -> Self {
        Self {
            error,
            model: model.to_owned(),
            first_model: None,
            retry_reason: None,
        }
    }
}

/// The audio a transcription reads: always the WAV bytes, plus a file when one already exists
/// (local models read files; retries read the kept file).
pub(super) struct Audio {
    pub(super) wav: Vec<u8>,
    pub(super) path: Option<PathBuf>,
}

/// One model's result.
struct Run {
    text: String,
    language: Option<String>,
    quality: TranscriptQuality,
    ms: i64,
}

impl Run {
    fn heard(self, model: &str, first_model: Option<&str>, reason: Option<RetryReason>) -> Heard {
        Heard {
            text: self.text,
            language: self.language,
            ms: self.ms,
            model: model.to_owned(),
            first_model: first_model.map(str::to_owned),
            retry_reason: reason,
        }
    }
}

pub(super) async fn vocabulary(state: &AppState) -> Vocabulary {
    let entries = match state.db() {
        Ok(db) => db.dictionary().await.unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the dictionary");
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    Vocabulary::from_entries(&entries)
}

/// Local models read a file, so a fresh recording is written to its own per-session WAV
/// (overlapping sessions never share one) whenever a local model might run.
pub(super) async fn transcribe_fresh(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    id: u64,
    audio: &mut Audio,
) -> Result<Heard, Unheard> {
    if settings.transcription_model != GROQ_WHISPER {
        let path = write_session_wav(&audio.wav, id)
            .await
            .map_err(|error| Unheard::untried(settings, error))?;
        audio.path = Some(path);
    }
    transcribe(state, settings, vocabulary, audio).await
}

pub(super) async fn transcribe(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    audio: &Audio,
) -> Result<Heard, Unheard> {
    let model = settings.transcription_model.as_str();
    if model == SMART_SELECT {
        return smart_transcribe(state, settings, vocabulary, audio).await;
    }
    match run(state, settings, vocabulary, audio, model).await {
        Ok(result) => {
            tracing::info!(model, words = word_count(&result), quality = ?result.quality,
                "transcribed");
            Ok(result.heard(model, None, None))
        }
        Err(error) => {
            tracing::info!(model, %error, "transcription failed");
            Err(Unheard::from_model(model, error))
        }
    }
}

/// Runs the first ready model of the route, then at most one retry when it errored or was
/// unsure (see [`settle`] for which result wins).
async fn smart_transcribe(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    audio: &Audio,
) -> Result<Heard, Unheard> {
    let groq = groq_key(state)
        .await
        .map_err(|error| Unheard::untried(settings, error))?
        .is_some();
    let route = ready_route(state, settings, groq)
        .await
        .map_err(|error| Unheard::untried(settings, error))?;
    let first = route.first;
    let first_result = run(state, settings, vocabulary, audio, first).await;
    let reason = match &first_result {
        Ok(result) => smart_select::gate(&result.text, &result.quality, settings.smart_retry),
        Err(_) => Some(RetryReason::Failed),
    };
    let retry = reason.zip(route.retry);
    let Some((reason, second)) = retry else {
        log_choice(first, &first_result, None, None);
        return match first_result {
            Ok(result) => Ok(result.heard(first, None, None)),
            Err(error) => Err(Unheard::from_model(first, error)),
        };
    };

    let second_result = run(state, settings, vocabulary, audio, second).await;
    log_choice(
        first,
        &first_result,
        Some(reason),
        Some((second, &second_result)),
    );
    settle(first, first_result, reason, second, second_result)
}

/// Picks what a dictation that ran a retry keeps. A retry with text replaces the first result;
/// otherwise a successful first result stands; when both fail the retry's error is reported.
/// Either way history records both models and why the retry ran.
fn settle(
    first: &str,
    first_result: AppResult<Run>,
    reason: RetryReason,
    second: &str,
    second_result: AppResult<Run>,
) -> Result<Heard, Unheard> {
    match (second_result, first_result) {
        (Ok(retried), _) if !retried.text.trim().is_empty() => {
            Ok(retried.heard(second, Some(first), Some(reason)))
        }
        (_, Ok(kept)) => Ok(kept.heard(first, Some(first), Some(reason))),
        // The first model failed and the retry heard nothing, so nothing was said.
        (Ok(silent), Err(_)) => Ok(silent.heard(second, Some(first), Some(reason))),
        (Err(error), Err(_)) => Err(Unheard {
            error,
            model: second.to_owned(),
            first_model: Some(first.to_owned()),
            retry_reason: Some(reason),
        }),
    }
}

/// The route for the current languages, narrowed to models that can run right now.
async fn ready_route(state: &AppState, settings: &Settings, groq: bool) -> AppResult<ReadyRoute> {
    let route = smart_select::route(&settings.languages, groq);
    let mut ready = Vec::with_capacity(route.len());
    for &model in &route {
        if model == GROQ_WHISPER || is_ready(state, settings, model).await {
            ready.push(model);
        }
    }
    smart_select::ready_route(&route, |model| ready.contains(&model))
}

/// One line per Smart Select dictation: models, their quality numbers and why a retry ran.
/// Never the transcript.
fn log_choice(
    first: &str,
    first_result: &AppResult<Run>,
    reason: Option<RetryReason>,
    retry: Option<(&str, &AppResult<Run>)>,
) {
    let first_quality = first_result.as_ref().ok().map(|result| result.quality);
    let first_error = first_result.as_ref().err().map(ToString::to_string);
    let retry_model = retry.map(|(model, _)| model);
    let retry_quality = retry.and_then(|(_, result)| result.as_ref().ok().map(|r| r.quality));
    let retry_error = retry.and_then(|(_, result)| result.as_ref().err().map(ToString::to_string));
    tracing::info!(
        first,
        first_quality = ?first_quality,
        first_words = first_result.as_ref().ok().map(word_count),
        first_error,
        retry_reason = reason.map(RetryReason::as_str),
        retry_model,
        retry_quality = ?retry_quality,
        retry_error,
        "smart select"
    );
}

fn word_count(result: &Run) -> usize {
    sv_domain::text_stats::word_count(&result.text)
}

async fn run(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    audio: &Audio,
    model: &str,
) -> AppResult<Run> {
    let started = Instant::now();
    if model == GROQ_WHISPER {
        let key = groq_key(state)
            .await?
            .ok_or_else(|| AppError::invalid(NO_GROQ_KEY))?;
        let transcript = sv_cloud::groq_transcribe(
            &state.http,
            &key,
            audio.wav.clone(),
            &vocabulary.prompt,
            &settings.languages,
            &settings.fallback_language,
        )
        .await?;
        return Ok(Run {
            text: transcript.text,
            language: transcript.language,
            quality: transcript.quality,
            ms: elapsed_ms(started),
        });
    }
    let path = audio
        .path
        .as_deref()
        .ok_or_else(|| AppError::other("Audio file missing"))?;
    // Whisper detects among the allowed languages and is primed with the dictionary, like
    // Groq's; the other local models take neither.
    let whisper = model == WHISPER_TURBO || model == WHISPER_HINGLISH;
    let options = TranscribeOptions {
        language: engine_language(settings, model),
        languages: if whisper { &settings.languages } else { &[] },
        prompt: if whisper { &vocabulary.prompt } else { "" },
    };
    let transcript = state.engine.transcribe(model, path, options).await?;
    Ok(Run {
        text: transcript.text,
        language: transcript.language,
        quality: transcript.quality,
        ms: elapsed_ms(started),
    })
}

pub(super) async fn groq_key(state: &AppState) -> AppResult<Option<String>> {
    let key = state.db()?.groq_api_key().await?;
    Ok(key.filter(|key| !key.trim().is_empty()))
}

async fn write_session_wav(wav: &[u8], id: u64) -> AppResult<PathBuf> {
    let path = sv_storage::paths::audio_dir()?.join(format!("session-{id}-{}.wav", now_ms()));
    tokio::fs::write(&path, wav).await.map_err(AppError::io)?;
    Ok(path)
}

/// Makes sure a failed session's audio is on disk for retry and returns its path.
pub(super) async fn keep_audio(audio: &Audio, id: u64) -> Option<PathBuf> {
    if let Some(path) = &audio.path {
        return Some(path.clone());
    }
    match write_session_wav(&audio.wav, id).await {
        Ok(path) => Some(path),
        Err(error) => {
            tracing::error!(%error, "could not keep the audio of a failed dictation");
            None
        }
    }
}

pub(super) async fn discard_audio(audio: &Audio) {
    if let Some(path) = &audio.path {
        remove_audio(path).await;
    }
}

#[cfg(test)]
mod tests {
    use sv_domain::models::{GROQ_WHISPER, PARAKEET_TDT_V2, WHISPER_HINGLISH};

    use super::*;

    fn heard(text: &str) -> AppResult<Run> {
        Ok(Run {
            text: text.to_owned(),
            language: Some("en".to_owned()),
            quality: TranscriptQuality::default(),
            ms: 100,
        })
    }

    fn failed(message: &str) -> AppResult<Run> {
        Err(AppError::network(message))
    }

    fn route_of(result: &Result<Heard, Unheard>) -> (&str, Option<&str>, Option<RetryReason>) {
        match result {
            Ok(heard) => (
                &heard.model,
                heard.first_model.as_deref(),
                heard.retry_reason,
            ),
            Err(unheard) => (
                &unheard.model,
                unheard.first_model.as_deref(),
                unheard.retry_reason,
            ),
        }
    }

    #[test]
    fn a_retry_with_text_replaces_the_first_result() {
        let low = RetryReason::LowConfidence;
        let settled = settle(
            WHISPER_HINGLISH,
            heard("kal"),
            low,
            PARAKEET_TDT_V2,
            heard("call"),
        );
        assert_eq!(
            route_of(&settled),
            (PARAKEET_TDT_V2, Some(WHISPER_HINGLISH), Some(low))
        );
        assert_eq!(
            settled.ok().map(|heard| heard.text).as_deref(),
            Some("call")
        );

        let offline = RetryReason::Failed;
        let settled = settle(
            GROQ_WHISPER,
            failed("offline"),
            offline,
            WHISPER_HINGLISH,
            heard("hi"),
        );
        assert_eq!(
            route_of(&settled),
            (WHISPER_HINGLISH, Some(GROQ_WHISPER), Some(offline))
        );
    }

    #[test]
    fn a_failed_or_empty_retry_keeps_a_successful_first_result() {
        let low = RetryReason::LowConfidence;
        for retry in [failed("timeout"), heard("  ")] {
            let settled = settle(
                WHISPER_HINGLISH,
                heard("kal milte"),
                low,
                PARAKEET_TDT_V2,
                retry,
            );
            assert_eq!(
                route_of(&settled),
                (WHISPER_HINGLISH, Some(WHISPER_HINGLISH), Some(low))
            );
            assert_eq!(
                settled.ok().map(|heard| heard.text).as_deref(),
                Some("kal milte")
            );
        }
    }

    #[test]
    fn when_both_fail_the_retry_error_is_reported_against_the_last_model() {
        let settled = settle(
            GROQ_WHISPER,
            failed("offline"),
            RetryReason::Failed,
            WHISPER_HINGLISH,
            failed("model missing"),
        );
        assert_eq!(
            route_of(&settled),
            (
                WHISPER_HINGLISH,
                Some(GROQ_WHISPER),
                Some(RetryReason::Failed)
            )
        );
        let error = settled.err().map(|unheard| unheard.error);
        assert_eq!(error, Some(AppError::network("model missing")));
    }

    #[test]
    fn a_silent_retry_after_a_failure_is_no_speech() {
        let settled = settle(
            GROQ_WHISPER,
            failed("offline"),
            RetryReason::Failed,
            WHISPER_HINGLISH,
            heard(""),
        );
        assert_eq!(settled.ok().map(|heard| heard.text).as_deref(), Some(""));
    }
}
