// FilePath: src-tauri/src/features/dictation/transcription.rs
//! Speech to text for dictations, their retries and edit instructions: Groq Whisper or the
//! engine helper's local models, the personal dictionary that primes them, and the session WAV
//! files the local models read.

use std::path::PathBuf;
use std::time::Instant;

use sv_domain::models::GROQ_WHISPER;
use sv_domain::{AppError, AppResult, Settings};
use sv_text::Vocabulary;

use super::elapsed_ms;
use crate::features::history::remove_audio;
use crate::features::models::engine_language;
use crate::state::{now_ms, AppState};

const NO_GROQ_KEY: &str = "Add your Groq API key in Settings → Models";

pub(super) struct Heard {
    pub(super) text: String,
    pub(super) language: Option<String>,
    /// Time the transcription model took, excluding recording and file writes.
    pub(super) ms: i64,
}

/// The audio a transcription reads: always the WAV bytes, plus a file when one already exists
/// (local models read files; retries read the kept file).
pub(super) struct Audio {
    pub(super) wav: Vec<u8>,
    pub(super) path: Option<PathBuf>,
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
/// (overlapping sessions never share one).
pub(super) async fn transcribe_fresh(
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

pub(super) async fn transcribe(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    audio: &Audio,
) -> AppResult<Heard> {
    let model = settings.transcription_model.as_str();
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
        return Ok(Heard {
            text: transcript.text,
            language: transcript.language,
            ms: elapsed_ms(started),
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
