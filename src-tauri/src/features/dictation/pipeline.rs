// FilePath: src-tauri/src/features/dictation/pipeline.rs
//! One finished recording → text in the frontmost app and a history row. Transcription runs on
//! Groq or the engine helper, then deterministic formatting, then the optional post-processing
//! pass, then ordered delivery. The same steps (minus pasting) retry a failed entry.

use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use sv_audio::{encode_wav, Cue, Recording};
use sv_cloud::ChatEndpoint;
use sv_domain::models::GROQ_WHISPER;
use sv_domain::text_stats::word_count;
use sv_domain::{
    AppError, AppResult, DictationPhase, DictationState, HistoryEntry, HistoryStatus, NewHistory,
    PostProcessing, Settings,
};
use sv_engine::FrontmostApp;
use sv_text::{PolishOutcome, Vocabulary};
use tauri::{AppHandle, Manager};

use super::delivery::{self, Delivery};
use super::{publish, publish_message, publish_phase};
use crate::events;
use crate::features::history::remove_audio;
use crate::features::models::engine_language;
use crate::state::{now_ms, AppState};

const NO_GROQ_KEY: &str = "Add your Groq API key in Settings → Models";
const UNFORMATTED: &str = "unformatted";
const TEST_SAMPLE: &str = "um so i think we should uh move the standup to ten tomorrow and \
     the the api review to friday and can you send the notes to priya";

pub(crate) struct SessionInput {
    pub(crate) id: u64,
    pub(crate) recording: Recording,
    pub(crate) settings: Settings,
    pub(crate) started_at_ms: i64,
    pub(crate) stopped_at: Instant,
    pub(crate) frontmost: Option<FrontmostApp>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostProcessingTest {
    output: String,
    latency_ms: i64,
}

struct Heard {
    text: String,
    language: Option<String>,
}

/// Result of the post-processing pass.
enum Polish {
    Polished(String),
    /// The pass failed or was rejected by the guards; the deterministic text is used.
    Unformatted(String),
    /// Off, or too short to be worth it.
    NotRun,
}

/// The audio a transcription reads: always the WAV bytes, plus a file when one already exists
/// (local models read files; retries read the kept file).
struct Audio {
    wav: Vec<u8>,
    path: Option<PathBuf>,
}

pub(crate) async fn run_session(app: AppHandle, input: SessionInput) {
    let SessionInput {
        id,
        recording,
        settings,
        started_at_ms,
        stopped_at,
        frontmost,
    } = input;
    let state = app.state::<AppState>();
    publish_phase(&app, id, DictationPhase::Transcribing);

    let db = match state.db() {
        Ok(db) => db.clone(),
        Err(error) => {
            state.play(&settings, Cue::Error);
            return publish_message(&app, id, DictationPhase::Error, error.to_string());
        }
    };
    let vocabulary = vocabulary(&state).await;
    let (app_name, bundle_id) =
        frontmost.map_or((None, None), |front| (front.name, front.bundle_id));
    let mut row = NewHistory {
        created_at: started_at_ms,
        status: HistoryStatus::Failed,
        raw_text: String::new(),
        final_text: String::new(),
        error: None,
        model: settings.transcription_model.clone(),
        language: None,
        style: settings.style,
        audio_ms: recording.duration_ms,
        latency_ms: 0,
        dictionary_fixes: 0,
        app_name,
        bundle_id,
        audio_path: None,
    };

    let mut audio = Audio {
        wav: encode_wav(&recording.samples),
        path: None,
    };
    let heard = transcribe_fresh(&state, &settings, &vocabulary, id, &mut audio).await;
    let heard = match heard {
        Ok(heard) if heard.text.trim().is_empty() => {
            discard_audio(&audio).await;
            return publish_message(&app, id, DictationPhase::Cancelled, "No speech detected");
        }
        Ok(heard) => heard,
        Err(error) => {
            // Never lose a dictation: keep the audio and a retryable history row.
            let kept = keep_audio(&audio, id).await;
            row.error = Some(error.to_string());
            row.audio_path = kept.map(|path| path.to_string_lossy().into_owned());
            row.latency_ms = elapsed_ms(stopped_at);
            if let Err(db_error) = db.insert_history(row).await {
                tracing::error!(%db_error, "could not save the failed dictation");
            }
            events::history_changed(&app);
            state.play(&settings, Cue::Error);
            return publish_message(&app, id, DictationPhase::Error, error.to_string());
        }
    };

    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    if will_post_process(&settings, &formatted.text) {
        publish_phase(&app, id, DictationPhase::Formatting);
    }
    let polish = post_process(&state, &settings, &vocabulary.terms, &formatted.text).await;
    let (final_text, mut status, note) = match polish {
        Polish::Polished(text) => (text, HistoryStatus::Pasted, None),
        Polish::NotRun => (formatted.text, HistoryStatus::Pasted, None),
        Polish::Unformatted(reason) => {
            tracing::info!(reason, "post-processing skipped; pasting formatted text");
            (
                formatted.text,
                HistoryStatus::Unformatted,
                Some(UNFORMATTED.to_owned()),
            )
        }
    };

    let delivered =
        delivery::deliver_in_order(&app, id, &final_text, settings.restore_clipboard).await;
    row.latency_ms = elapsed_ms(stopped_at);
    row.raw_text = heard.text;
    row.language = heard.language;
    row.dictionary_fixes = i64::from(formatted.rule_hits);
    match &delivered {
        Delivery::Pasted => {}
        Delivery::Dropped => status = HistoryStatus::Dropped,
        Delivery::NotPasted(message) => {
            status = HistoryStatus::NotPasted;
            row.error = Some(message.clone());
        }
    }
    row.status = status;
    row.final_text = final_text;
    let words = i64::try_from(word_count(&row.final_text)).unwrap_or(i64::MAX);
    if let Err(error) = db.insert_history(row).await {
        tracing::error!(%error, "could not save the dictation to history");
    }
    discard_audio(&audio).await;
    events::history_changed(&app);

    match delivered {
        Delivery::Pasted => {
            let mut done = DictationState::new(DictationPhase::Done, id);
            done.words = Some(words);
            done.note = note;
            publish(&app, done);
        }
        Delivery::Dropped => {
            tracing::info!(
                session = id,
                "a newer dictation was already pasted; kept in history"
            );
        }
        Delivery::NotPasted(message) => {
            state.play(&settings, Cue::Error);
            publish_message(&app, id, DictationPhase::Error, message);
        }
    }
}

/// Re-runs a failed dictation from its kept audio. The app it was meant for is gone, so the
/// result is copied to the clipboard instead of pasted.
pub(crate) async fn retry(app: &AppHandle, id: i64) -> AppResult<HistoryEntry> {
    let state = app.state::<AppState>();
    let db = state.db()?.clone();
    let entry = db
        .history_entry(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Dictation {id}")))?;
    let path = db
        .history_audio_path(id)
        .await?
        .ok_or_else(|| AppError::invalid("This dictation has no saved audio to retry"))?;
    let wav = tokio::fs::read(&path).await.map_err(AppError::io)?;
    if sv_audio::decode_wav(&wav)?.is_empty() {
        return Err(AppError::invalid("The saved audio is empty"));
    }
    let settings = db.settings().await?;
    let vocabulary = vocabulary(&state).await;
    let started = Instant::now();
    let mut row = NewHistory {
        created_at: entry.created_at,
        status: HistoryStatus::Failed,
        raw_text: entry.raw_text,
        final_text: entry.text,
        error: None,
        model: settings.transcription_model.clone(),
        language: entry.language,
        style: settings.style,
        audio_ms: entry.audio_ms,
        latency_ms: entry.latency_ms,
        dictionary_fixes: entry.dictionary_fixes,
        app_name: entry.app_name,
        bundle_id: entry.bundle_id,
        audio_path: Some(path.clone()),
    };

    let audio = Audio {
        wav,
        path: Some(PathBuf::from(&path)),
    };
    let heard = match transcribe(&state, &settings, &vocabulary, &audio).await {
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
            events::history_changed(app);
            return Err(error);
        }
    };

    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    let polish = post_process(&state, &settings, &vocabulary.terms, &formatted.text).await;
    let final_text = match polish {
        Polish::Polished(text) => text,
        Polish::NotRun | Polish::Unformatted(_) => formatted.text,
    };
    // A retry only copies: the app the dictation was meant for is no longer focused.
    if let Err(error) = delivery::copy_to_clipboard(&final_text) {
        tracing::warn!(%error, "could not copy the retried text");
        row.error = Some(format!("Could not copy to the clipboard: {error}"));
    }
    row.status = HistoryStatus::NotPasted;
    row.raw_text = heard.text;
    row.final_text = final_text;
    row.language = heard.language;
    row.dictionary_fixes = i64::from(formatted.rule_hits);
    row.latency_ms = elapsed_ms(started);
    row.audio_path = None;
    let updated = db.update_history(id, row).await?;
    remove_audio(&path).await;
    events::history_changed(app);
    Ok(updated)
}

/// Formats a fixed sample with the configured provider so Settings can show it works.
pub(crate) async fn test_post_processing(app: &AppHandle) -> AppResult<PostProcessingTest> {
    let state = app.state::<AppState>();
    let settings = state.db()?.settings().await?;
    let vocabulary = vocabulary(&state).await;
    let formatted = sv_text::format(TEST_SAMPLE, &vocabulary, settings.style);
    let started = Instant::now();
    let output = match post_process(&state, &settings, &vocabulary.terms, &formatted.text).await {
        Polish::Polished(text) => text,
        Polish::NotRun => formatted.text,
        Polish::Unformatted(reason) => {
            return Err(AppError::other(format!("Post-processing failed: {reason}")));
        }
    };
    Ok(PostProcessingTest {
        output,
        latency_ms: elapsed_ms(started),
    })
}

async fn vocabulary(state: &AppState) -> Vocabulary {
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
/// (overlapping sessions never share one).
async fn transcribe_fresh(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    id: u64,
    audio: &mut Audio,
) -> AppResult<Heard> {
    if settings.transcription_model != GROQ_WHISPER {
        audio.path = Some(write_session_wav(&audio.wav, id).await?);
    }
    transcribe(state, settings, vocabulary, audio).await
}

async fn transcribe(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    audio: &Audio,
) -> AppResult<Heard> {
    let model = settings.transcription_model.as_str();
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
        return Ok(Heard {
            text: transcript.text,
            language: transcript.language,
        });
    }
    let path = audio
        .path
        .as_deref()
        .ok_or_else(|| AppError::other("Audio file missing"))?;
    let language = engine_language(settings, model);
    let transcript = state.engine.transcribe(model, path, language).await?;
    Ok(Heard {
        text: transcript.text,
        language: transcript.language,
    })
}

async fn groq_key(state: &AppState) -> AppResult<Option<String>> {
    let key = state.db()?.groq_api_key().await?;
    Ok(key.filter(|key| !key.trim().is_empty()))
}

fn will_post_process(settings: &Settings, text: &str) -> bool {
    settings.post_processing != PostProcessing::Off
        && !text.trim().is_empty()
        && sv_text::should_skip_polish(text).is_none()
}

async fn post_process(
    state: &AppState,
    settings: &Settings,
    terms: &[String],
    text: &str,
) -> Polish {
    if !will_post_process(settings, text) {
        return Polish::NotRun;
    }
    let prompt = sv_text::polish_prompt(text, terms, settings.style);
    let outcome = match settings.post_processing {
        PostProcessing::Off => return Polish::NotRun,
        PostProcessing::Groq => match groq_key(state).await {
            Ok(Some(key)) => {
                let endpoint = ChatEndpoint {
                    url: sv_cloud::GROQ_CHAT_URL.to_owned(),
                    key: Some(&key),
                    model: &settings.groq_formatting_model,
                    groq_no_reasoning: true,
                };
                sv_cloud::polish_chat(&state.http, endpoint, &prompt, text).await
            }
            Ok(None) => PolishOutcome::Skipped("no Groq API key".to_owned()),
            Err(error) => PolishOutcome::Skipped(error.to_string()),
        },
        PostProcessing::Custom => {
            if settings.custom_model.trim().is_empty() {
                PolishOutcome::Skipped("no custom model configured".to_owned())
            } else {
                let key = match state.db() {
                    Ok(db) => db.custom_api_key().await.ok().flatten(),
                    Err(_) => None,
                };
                let endpoint = ChatEndpoint {
                    url: sv_cloud::chat_url(&settings.custom_base_url),
                    key: key.as_deref().filter(|key| !key.trim().is_empty()),
                    model: &settings.custom_model,
                    groq_no_reasoning: false,
                };
                sv_cloud::polish_chat(&state.http, endpoint, &prompt, text).await
            }
        }
        PostProcessing::Apple => {
            let request = state
                .engine
                .polish(&prompt.system, &prompt.shots, &prompt.user);
            match tokio::time::timeout(sv_text::polish_timeout(text), request).await {
                Ok(Ok(reply)) => sv_text::accept_polish(text, &reply.text, reply.finished),
                Ok(Err(error)) => PolishOutcome::Skipped(error.to_string()),
                Err(_) => PolishOutcome::Skipped("timed out".to_owned()),
            }
        }
    };
    match outcome {
        PolishOutcome::Polished(polished) => Polish::Polished(polished),
        PolishOutcome::Skipped(reason) => Polish::Unformatted(reason),
    }
}

async fn write_session_wav(wav: &[u8], id: u64) -> AppResult<PathBuf> {
    let path = sv_storage::paths::audio_dir()?.join(format!("session-{id}-{}.wav", now_ms()));
    tokio::fs::write(&path, wav).await.map_err(AppError::io)?;
    Ok(path)
}

/// Makes sure a failed session's audio is on disk for retry and returns its path.
async fn keep_audio(audio: &Audio, id: u64) -> Option<PathBuf> {
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

async fn discard_audio(audio: &Audio) {
    if let Some(path) = &audio.path {
        remove_audio(path).await;
    }
}

fn elapsed_ms(since: Instant) -> i64 {
    i64::try_from(since.elapsed().as_millis()).unwrap_or(i64::MAX)
}
