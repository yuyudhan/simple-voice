// FilePath: src-tauri/src/features/meetings/processing.rs
//! Turning a recorded meeting into a transcript: mix the microphone and system audio, transcribe
//! bounded windows with timestamps, tell the speakers apart with the local diarization model,
//! give every segment its speaker, and store the result. Any stage that cannot run fails the
//! meeting with the reason; the audio is always kept so the meeting can be retried.

use std::path::{Path, PathBuf};

use sv_domain::languages::model_languages;
use sv_domain::{AppError, AppResult, Meeting, MeetingJob, MeetingStatus};
use sv_engine::{DiarizeOptions, TranscribeOptions};
use sv_storage::NewSegment;
use sv_text::meetings::{
    assign_speakers, audio_windows, format_timestamp, merge_window, SpeakerSpan, TimedText,
};
use tauri::{AppHandle, Manager};

use super::{
    begin_job, blocking, cancellable, changed, emit_meeting, emit_progress, is_cancelled,
    readiness, recorded_duration, system_offset_ms, transcription_model, MIC_FILE, MIXED_FILE,
    SAMPLES_PER_MS, SYSTEM_FILE, WINDOW_FILE,
};
use crate::features::models;
use crate::state::AppState;

const CANCELLED_NOTICE: &str =
    "Processing was cancelled. The audio is kept: choose Retry to process it.";

/// Marks the meeting `processing` and transcribes it in the background.
pub(super) async fn start(app: &AppHandle, id: i64) -> AppResult<Meeting> {
    let (guard, cancelled) = begin_job(app, id, MeetingJob::Transcribe)?;
    let state = app.state::<AppState>();
    let meeting = state
        .db()?
        .set_meeting_status(id, MeetingStatus::Processing, None)
        .await?;
    emit_meeting(app, &meeting);
    changed(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = cancellable(cancelled, process(&app, id)).await;
        drop(guard);
        finish(&app, id, outcome).await;
    });
    Ok(meeting)
}

async fn finish(app: &AppHandle, id: i64, outcome: AppResult<Meeting>) {
    let state = app.state::<AppState>();
    let db = match state.db() {
        Ok(db) => db,
        Err(error) => {
            tracing::warn!(%error, meeting = id, "database unavailable after processing");
            return;
        }
    };
    let stored = match outcome {
        Ok(meeting) => {
            tracing::info!(
                meeting = id,
                segments = meeting.segment_count,
                speakers = meeting.speaker_count,
                "meeting processed"
            );
            Ok(meeting)
        }
        Err(error) if is_cancelled(&error) => {
            tracing::info!(meeting = id, "meeting processing cancelled");
            db.set_meeting_status(id, MeetingStatus::Cancelled, Some(CANCELLED_NOTICE))
                .await
        }
        Err(error) => {
            tracing::warn!(%error, meeting = id, "meeting processing failed");
            let reason = error.to_string();
            db.set_meeting_status(id, MeetingStatus::Failed, Some(&reason))
                .await
        }
    };
    match stored {
        Ok(meeting) => emit_meeting(app, &meeting),
        // The meeting was deleted while it was processed.
        Err(error) => tracing::info!(%error, meeting = id, "processed meeting not stored"),
    }
    changed(app);
}

async fn process(app: &AppHandle, id: i64) -> AppResult<Meeting> {
    let state = app.state::<AppState>();
    let db = state.db()?;
    let settings = db.settings().await?;
    let ready = readiness(&state, &settings).await;
    if let Some(reason) = ready.transcription.or(ready.diarization) {
        return Err(AppError::Other(reason));
    }
    let model = transcription_model(&state, &settings).await;
    let meeting = db.meeting(id).await?;
    let dir = PathBuf::from(db.meeting_audio_dir(id).await?);
    let source = prepare_audio(&dir, meeting.started_at, meeting.system_audio).await?;

    let duration_ms = {
        let source = source.clone();
        sv_audio::samples_to_ms(blocking(move || sv_audio::wav_sample_count(&source)).await?)
    };
    if duration_ms == 0 {
        return Err(AppError::Audio("Nothing was recorded.".to_owned()));
    }

    // Transcribe bounded windows so no request outlasts the helper's limit and no reply
    // holds more than a few minutes of text.
    let windows = audio_windows(duration_ms);
    let codes = model_languages(&settings.languages);
    let window_path = dir.join(WINDOW_FILE);
    let mut transcript: Vec<TimedText> = Vec::new();
    let mut detected: Vec<(String, usize)> = Vec::new();
    for (index, window) in windows.iter().enumerate() {
        emit_progress(
            app,
            id,
            MeetingJob::Transcribe,
            Some(index as f32 / windows.len() as f32),
            Some(format!(
                "Transcribing {} of {}",
                format_timestamp(window.start_ms),
                format_timestamp(duration_ms)
            )),
        );
        let (from, to, out) = (
            ms_to_samples(window.start_ms),
            ms_to_samples(window.end_ms),
            window_path.clone(),
        );
        let input = source.clone();
        blocking(move || sv_audio::copy_wav_range(&input, from, to, &out)).await?;
        let options = TranscribeOptions {
            language: models::engine_language(&settings, &model),
            languages: &codes,
            prompt: "",
            vocabulary: &[],
            timestamps: true,
        };
        let result = state
            .engine
            .transcribe(&model, &window_path, options)
            .await
            .map_err(|error| AppError::Engine(format!("Transcription failed: {error}")))?;
        if let Some(language) = result.language.filter(|language| !language.is_empty()) {
            match detected.iter_mut().find(|(known, _)| *known == language) {
                Some((_, count)) => *count += 1,
                None => detected.push((language, 1)),
            }
        }
        let segments = result
            .segments
            .into_iter()
            .map(|segment| TimedText {
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                text: segment.text,
            })
            .collect();
        merge_window(&mut transcript, window, &result.text, segments);
    }
    remove_file(&window_path).await;
    emit_progress(app, id, MeetingJob::Transcribe, Some(1.0), None);
    if transcript.is_empty() {
        return Err(AppError::Other(
            "No speech was recognized in this recording.".to_owned(),
        ));
    }

    emit_progress(
        app,
        id,
        MeetingJob::Diarize,
        None,
        Some("Telling speakers apart".to_owned()),
    );
    let diarization = state
        .engine
        .diarize(
            &source,
            DiarizeOptions {
                speakers: None,
                max_speakers: None,
            },
        )
        .await
        .map_err(|error| AppError::Engine(format!("Speaker diarization failed: {error}")))?;
    let turns: Vec<SpeakerSpan> = diarization
        .segments
        .into_iter()
        .map(|turn| SpeakerSpan {
            start_ms: turn.start_ms,
            end_ms: turn.end_ms,
            speaker: turn.speaker,
        })
        .collect();
    let attribution = assign_speakers(transcript, &turns);
    let segments: Vec<NewSegment> = attribution
        .segments
        .into_iter()
        .map(|segment| NewSegment {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            speaker: segment.speaker,
            text: segment.text,
        })
        .collect();
    let language = detected
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(language, _)| language);
    let stored = db
        .replace_meeting_transcript(id, &model, language.as_deref(), &segments)
        .await?;
    emit_progress(app, id, MeetingJob::Diarize, Some(1.0), None);
    if source.ends_with(MIXED_FILE) {
        remove_file(&source).await;
    }
    Ok(stored)
}

fn ms_to_samples(ms: i64) -> u64 {
    u64::try_from(ms.saturating_mul(SAMPLES_PER_MS)).unwrap_or(0)
}

/// The file to transcribe: the microphone alone, or the microphone mixed with system audio
/// lined up by their start times.
async fn prepare_audio(dir: &Path, mic_started_at: i64, system_audio: bool) -> AppResult<PathBuf> {
    let mic = dir.join(MIC_FILE);
    if !mic.is_file() {
        return Err(AppError::Audio(
            "The meeting's microphone recording is missing.".to_owned(),
        ));
    }
    let system = dir.join(SYSTEM_FILE);
    if !system_audio || !system.is_file() {
        return Ok(mic);
    }
    let offset = system_offset_ms(dir, mic_started_at)
        .await
        .saturating_mul(SAMPLES_PER_MS);
    let mixed = dir.join(MIXED_FILE);
    let out = mixed.clone();
    blocking(move || sv_audio::mix_wav_files(&mic, &system, offset, &out)).await?;
    Ok(mixed)
}

async fn remove_file(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%error, path = %path.display(), "could not remove a meeting work file");
        }
    }
}

/// Startup recovery: meetings a quit or crash interrupted are marked failed (audio kept for a
/// retry). Recordings cut off mid-way get their WAV headers repaired and their length measured.
pub(crate) fn spawn_recovery(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let db = match state.db() {
            Ok(db) => db,
            Err(error) => {
                tracing::warn!(%error, "meeting recovery skipped: database unavailable");
                return;
            }
        };
        let recovered = match db.recover_interrupted_meetings().await {
            Ok(recovered) => recovered,
            Err(error) => {
                tracing::warn!(%error, "meeting recovery failed");
                return;
            }
        };
        if recovered.is_empty() {
            return;
        }
        for meeting in &recovered {
            if meeting.ended_at.is_some() {
                continue;
            }
            let dir = match db.meeting_audio_dir(meeting.id).await {
                Ok(dir) => PathBuf::from(dir),
                Err(error) => {
                    tracing::warn!(%error, meeting = meeting.id, "recovered meeting has no audio");
                    continue;
                }
            };
            for name in [MIC_FILE, SYSTEM_FILE] {
                repair_wav(&dir.join(name)).await;
            }
            let duration_ms = recorded_duration(&dir, meeting.started_at).await;
            let ended_at = meeting.started_at.saturating_add(duration_ms);
            if let Err(error) = db
                .finish_meeting_recording(meeting.id, ended_at, duration_ms)
                .await
            {
                tracing::warn!(%error, meeting = meeting.id, "could not store recovered length");
            }
        }
        tracing::info!(
            count = recovered.len(),
            "interrupted meetings marked failed"
        );
        changed(&app);
    });
}

/// Rewrites a WAV whose header was never finalized so every reader (the helper too) sees all
/// of its samples. Streams the copy; a missing file is left alone.
async fn repair_wav(path: &Path) {
    if !path.is_file() {
        return;
    }
    let source = path.to_path_buf();
    let repaired = path.with_extension("repaired.wav");
    let target = repaired.clone();
    let copied = blocking(move || {
        sv_audio::copy_wav_range(&source, 0, u64::MAX, &target)?;
        std::fs::rename(&target, &source)?;
        Ok(())
    })
    .await;
    if let Err(error) = copied {
        tracing::warn!(%error, path = %path.display(), "could not repair an interrupted recording");
        remove_file(&repaired).await;
    }
}
