// FilePath: crates/sv-mobile/src/pipeline.rs
//! One finished phone recording → text for the keyboard and a history row. The same steps as
//! the desktop pipeline minus pasting: Groq Whisper, the deterministic rules, then the Groq
//! formatting pass. A failed transcription keeps its audio so it can be retried (D-14).

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;
use sv_cloud::ChatEndpoint;
use sv_domain::models::GROQ_WHISPER;
use sv_domain::{
    AppError, AppResult, HistoryEntry, HistoryStatus, NewHistory, PostProcessing, Settings,
};
use sv_storage::Db;
use sv_text::{PolishOutcome, PolishTarget, Vocabulary};

use crate::status::NO_GROQ_KEY;

/// Formatting only shortens a transcript, so its reply fits comfortably.
const POLISH_MAX_TOKENS: u32 = 2048;
/// The smallest WAV file that holds any audio: a 44-byte header plus samples.
const WAV_HEADER_BYTES: usize = 44;

/// A recording the keyboard finished and wrote to `wav_path` (16 kHz mono PCM16 WAV).
#[derive(Debug, Clone)]
pub struct Recording {
    pub wav_path: PathBuf,
    pub duration_ms: i64,
    pub app_name: Option<String>,
    /// The Android package (or iOS bundle id) of the app being typed in.
    pub app_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Dictation {
    /// Text for the cursor, ending in whitespace so dictations never run together (D-16).
    /// `id` is its history row, for [`Db::mark_history_not_pasted`] if it never lands.
    #[serde(rename_all = "camelCase")]
    Text {
        id: i64,
        text: String,
        unformatted: bool,
    },
    /// Nothing was said; nothing is kept.
    NoSpeech,
    /// Transcription failed. The audio is kept and `id` (when the row was saved) can be retried.
    Failed { id: Option<i64>, message: String },
}

/// How a retried dictation reached the user, which decides its history status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Inserted at the cursor by the keyboard.
    Inserted,
    /// Copied to the clipboard by the app, the field it was meant for being gone.
    Copied,
}

pub(crate) struct Heard {
    pub(crate) text: String,
    pub(crate) language: Option<String>,
    pub(crate) ms: i64,
}

pub async fn dictate(db: &Db, http: &reqwest::Client, recording: Recording) -> Dictation {
    let started = Instant::now();
    let created_at = chrono::Utc::now().timestamp_millis();
    let wav_path = recording.wav_path.to_string_lossy().into_owned();
    let settings = match db.settings().await {
        Ok(settings) => settings,
        Err(error) => return failed(None, error),
    };
    let mut row = NewHistory {
        created_at,
        status: HistoryStatus::Failed,
        raw_text: String::new(),
        final_text: String::new(),
        source_text: None,
        error: None,
        model: GROQ_WHISPER.to_owned(),
        format_model: None,
        language: None,
        style: settings.style,
        audio_ms: recording.duration_ms,
        latency_ms: 0,
        transcribe_ms: None,
        format_ms: None,
        dictionary_fixes: 0,
        app_name: recording.app_name,
        bundle_id: recording.app_id,
        audio_path: None,
        // Phones run Groq Whisper only, so Smart Select never retries on a second model.
        first_model: None,
        retry_reason: None,
    };

    let vocabulary = vocabulary(db).await;
    let heard = match read_wav(&recording.wav_path).await {
        Ok(wav) => transcribe(db, http, &settings, &vocabulary, wav).await,
        Err(error) => Err(error),
    };
    let heard = match heard {
        Ok(heard) if heard.text.trim().is_empty() => {
            remove_audio(&recording.wav_path).await;
            return Dictation::NoSpeech;
        }
        Ok(heard) => heard,
        Err(error) => {
            // Never lose a dictation: keep the audio and a retryable history row.
            row.error = Some(error.to_string());
            row.audio_path = Some(wav_path);
            row.latency_ms = elapsed_ms(started);
            let id = match db.insert_history(row).await {
                Ok(entry) => Some(entry.id),
                Err(db_error) => {
                    tracing::error!(%db_error, "could not save the failed dictation");
                    None
                }
            };
            return failed(id, error);
        }
    };

    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    let (final_text, unformatted) =
        match post_process(db, http, &settings, &vocabulary.terms, &formatted.text).await {
            Polish::Polished(text, ms) => {
                row.format_model = Some(settings.groq_formatting_model.clone());
                row.format_ms = Some(ms);
                row.status = HistoryStatus::Pasted;
                (text, false)
            }
            Polish::NotRun => {
                row.status = HistoryStatus::Pasted;
                (formatted.text, false)
            }
            Polish::Unformatted(reason) => {
                row.status = HistoryStatus::Unformatted;
                row.error = Some(reason);
                (formatted.text, true)
            }
        };
    row.raw_text = heard.text;
    row.language = heard.language;
    row.transcribe_ms = Some(heard.ms);
    row.dictionary_fixes = i64::from(formatted.rule_hits);
    row.final_text = final_text.clone();
    row.latency_ms = elapsed_ms(started);
    let saved = db.insert_history(row).await;
    remove_audio(&recording.wav_path).await;
    match saved {
        Ok(entry) => Dictation::Text {
            id: entry.id,
            text: with_separator(&final_text),
            unformatted,
        },
        Err(error) => {
            // The text is still worth inserting; only the history row is missing.
            tracing::error!(%error, "could not save the dictation to history");
            Dictation::Text {
                id: 0,
                text: with_separator(&final_text),
                unformatted,
            }
        }
    }
}

/// Re-runs a failed dictation from its kept audio. The caller delivers the returned text
/// (ending in whitespace when `delivery` is [`Delivery::Inserted`]).
pub async fn retry(
    db: &Db,
    http: &reqwest::Client,
    id: i64,
    delivery: Delivery,
) -> AppResult<(HistoryEntry, String)> {
    let entry = db
        .history_entry(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Dictation {id}")))?;
    let path = db
        .history_audio_path(id)
        .await?
        .ok_or_else(|| AppError::invalid("This dictation has no saved audio to retry"))?;
    let wav = read_wav(Path::new(&path)).await?;
    let settings = db.settings().await?;
    let vocabulary = vocabulary(db).await;
    let started = Instant::now();
    let mut row = NewHistory {
        created_at: entry.created_at,
        status: HistoryStatus::Failed,
        raw_text: entry.raw_text,
        final_text: entry.text,
        source_text: None,
        error: None,
        model: GROQ_WHISPER.to_owned(),
        format_model: None,
        language: entry.language,
        style: settings.style,
        audio_ms: entry.audio_ms,
        latency_ms: entry.latency_ms,
        transcribe_ms: None,
        format_ms: None,
        dictionary_fixes: entry.dictionary_fixes,
        app_name: entry.app_name,
        bundle_id: entry.bundle_id,
        audio_path: Some(path.clone()),
        first_model: None,
        retry_reason: None,
    };

    let heard = match transcribe(db, http, &settings, &vocabulary, wav).await {
        Ok(heard) if heard.text.trim().is_empty() => {
            Err(AppError::other("No speech was detected in the saved audio"))
        }
        other => other,
    };
    let heard = match heard {
        Ok(heard) => heard,
        Err(error) => {
            row.error = Some(error.to_string());
            db.update_history(id, row).await?;
            return Err(error);
        }
    };

    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    let final_text =
        match post_process(db, http, &settings, &vocabulary.terms, &formatted.text).await {
            Polish::Polished(text, ms) => {
                row.format_model = Some(settings.groq_formatting_model.clone());
                row.format_ms = Some(ms);
                text
            }
            Polish::NotRun | Polish::Unformatted(_) => formatted.text,
        };
    row.status = match delivery {
        Delivery::Inserted => HistoryStatus::Pasted,
        Delivery::Copied => HistoryStatus::NotPasted,
    };
    row.raw_text = heard.text;
    row.language = heard.language;
    row.transcribe_ms = Some(heard.ms);
    row.dictionary_fixes = i64::from(formatted.rule_hits);
    row.final_text = final_text.clone();
    row.latency_ms = elapsed_ms(started);
    row.audio_path = None;
    let updated = db.update_history(id, row).await?;
    remove_audio(Path::new(&path)).await;
    let text = match delivery {
        Delivery::Inserted => with_separator(&final_text),
        Delivery::Copied => final_text,
    };
    Ok((updated, text))
}

enum Polish {
    Polished(String, i64),
    NotRun,
    Unformatted(String),
}

pub(crate) async fn transcribe(
    db: &Db,
    http: &reqwest::Client,
    settings: &Settings,
    vocabulary: &Vocabulary,
    wav: Vec<u8>,
) -> AppResult<Heard> {
    let key = groq_key(db)
        .await?
        .ok_or_else(|| AppError::invalid(NO_GROQ_KEY))?;
    let started = Instant::now();
    // Models know only spoken languages: script variants go as their base (as on the desktop).
    let languages = sv_domain::languages::model_languages(&settings.languages);
    let transcript = sv_cloud::groq_transcribe(
        http,
        &key,
        wav,
        &sv_text::whisper_prompt(&settings.languages, vocabulary),
        &languages,
        &settings.fallback_language,
    )
    .await?;
    Ok(Heard {
        text: transcript.text,
        language: transcript.language,
        ms: elapsed_ms(started),
    })
}

/// The phone apps format with Groq only; the desktop's other providers do not exist there.
async fn post_process(
    db: &Db,
    http: &reqwest::Client,
    settings: &Settings,
    terms: &[String],
    text: &str,
) -> Polish {
    if settings.post_processing != PostProcessing::Groq
        || text.trim().is_empty()
        || sv_text::should_skip_polish(text).is_some()
    {
        return Polish::NotRun;
    }
    let key = match groq_key(db).await {
        Ok(Some(key)) => key,
        Ok(None) => return Polish::Unformatted("no Groq API key".to_owned()),
        Err(error) => return Polish::Unformatted(error.to_string()),
    };
    let prompt = sv_text::polish_prompt(
        text,
        terms,
        settings.style,
        PolishTarget::Chat,
        &settings.languages,
    );
    let endpoint = ChatEndpoint {
        url: sv_cloud::GROQ_CHAT_URL.to_owned(),
        key: Some(&key),
        model: &settings.groq_formatting_model,
        groq_no_reasoning: true,
    };
    let started = Instant::now();
    let limit = sv_text::polish_timeout(text);
    let outcome =
        match sv_cloud::chat_completion(http, &endpoint, &prompt, POLISH_MAX_TOKENS, limit).await {
            Ok(reply) => sv_text::accept_polish(text, &reply.text, reply.finished),
            Err(reason) => PolishOutcome::Skipped(reason),
        };
    match outcome {
        PolishOutcome::Polished(polished) => Polish::Polished(polished, elapsed_ms(started)),
        PolishOutcome::Skipped(reason) => Polish::Unformatted(reason),
    }
}

pub(crate) async fn groq_key(db: &Db) -> AppResult<Option<String>> {
    Ok(db
        .groq_api_key()
        .await?
        .filter(|key| !key.trim().is_empty()))
}

pub(crate) async fn vocabulary(db: &Db) -> Vocabulary {
    let entries = db.dictionary().await.unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the dictionary");
        Vec::new()
    });
    Vocabulary::from_entries(&entries)
}

pub(crate) async fn read_wav(path: &Path) -> AppResult<Vec<u8>> {
    let wav = tokio::fs::read(path).await.map_err(AppError::io)?;
    if wav.len() <= WAV_HEADER_BYTES {
        return Err(AppError::invalid("The recording is empty"));
    }
    Ok(wav)
}

/// Removes a recording. A file that is already gone is the desired end state.
pub(crate) async fn remove_audio(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, path = %path.display(), "could not delete audio"),
    }
}

fn failed(id: Option<i64>, error: AppError) -> Dictation {
    Dictation::Failed {
        id,
        message: error.to_string(),
    }
}

/// Appends a space unless the text already ends in whitespace (D-16).
pub(crate) fn with_separator(text: &str) -> String {
    if text.is_empty() || text.ends_with(char::is_whitespace) {
        return text.to_owned();
    }
    format!("{text} ")
}

pub(crate) fn elapsed_ms(since: Instant) -> i64 {
    i64::try_from(since.elapsed().as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV header and one second of silence: enough for the pipeline to try it.
    fn write_wav(dir: &Path) -> PathBuf {
        let path = dir.join("session.wav");
        std::fs::write(&path, vec![0_u8; WAV_HEADER_BYTES + 32_000]).unwrap();
        path
    }

    async fn open_without_key() -> Option<(tempfile::TempDir, Db)> {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        // GROQ_API_KEY in the environment would reach the network; these cases need no key.
        db.groq_api_key()
            .await
            .unwrap()
            .is_none()
            .then_some((dir, db))
    }

    #[tokio::test]
    async fn a_failed_dictation_keeps_its_audio_and_a_retryable_row() {
        let Some((dir, db)) = open_without_key().await else {
            return;
        };
        let wav_path = write_wav(dir.path());
        let recording = Recording {
            wav_path: wav_path.clone(),
            duration_ms: 1_000,
            app_name: Some("Messages".to_owned()),
            app_id: Some("com.google.android.apps.messaging".to_owned()),
        };

        let outcome = dictate(&db, &http_client(), recording).await;
        let Dictation::Failed {
            id: Some(id),
            message,
        } = outcome
        else {
            panic!("expected a saved failure, got {outcome:?}");
        };
        assert_eq!(message, NO_GROQ_KEY);
        assert!(wav_path.exists(), "the audio must survive for Retry");
        let entry = db.history_entry(id).await.unwrap().unwrap();
        assert_eq!(entry.status, HistoryStatus::Failed);
        assert!(entry.can_retry);
        assert_eq!(
            entry.bundle_id.as_deref(),
            Some("com.google.android.apps.messaging")
        );

        // Retrying without a key fails the same way and leaves the row retryable.
        let retried = retry(&db, &http_client(), id, Delivery::Inserted).await;
        assert!(matches!(retried, Err(AppError::InvalidInput(_))));
        assert!(db.history_entry(id).await.unwrap().unwrap().can_retry);
        assert!(wav_path.exists());
    }

    #[tokio::test]
    async fn an_empty_recording_fails_without_reaching_groq() {
        let Some((dir, db)) = open_without_key().await else {
            return;
        };
        let wav_path = dir.path().join("empty.wav");
        std::fs::write(&wav_path, vec![0_u8; WAV_HEADER_BYTES]).unwrap();
        let recording = Recording {
            wav_path,
            duration_ms: 0,
            app_name: None,
            app_id: None,
        };

        let outcome = dictate(&db, &http_client(), recording).await;
        assert!(matches!(
            outcome,
            Dictation::Failed { message, .. } if message == "The recording is empty"
        ));
    }

    #[test]
    fn inserted_text_always_ends_in_whitespace() {
        assert_eq!(with_separator("Ship it"), "Ship it ");
        assert_eq!(with_separator("- one\n- two\n"), "- one\n- two\n");
        assert_eq!(with_separator(""), "");
    }

    fn http_client() -> reqwest::Client {
        crate::http_client().unwrap()
    }
}
