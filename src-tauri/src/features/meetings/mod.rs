// FilePath: src-tauri/src/features/meetings/mod.rs
//! Personal meetings: an explicitly started recording of the microphone and, with consent,
//! system audio; transcribed window by window with timestamps by a local speech model, split
//! into speakers by the local diarization model, then summarized, questioned and translated by
//! the on-device meeting assistant. Everything runs through the engine helper on this Mac.
//!
//! One meeting records at a time ([`recording`]). Processing ([`processing`]) and the
//! assistant's jobs ([`assistant`]) run one per meeting and are cancelled by dropping their
//! futures, which cancels the helper request in flight. Logs never carry transcript text.

mod assistant;
mod jobs;
mod processing;
mod recording;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Mutex;

use chrono::TimeZone;
use sv_domain::models::{DIARIZER, TRANSLATOR};
use sv_domain::{
    AppError, AppResult, ChatMessage, ExportFormat, Meeting, MeetingDetail, MeetingJob,
    MeetingProgress, MeetingReadiness, MeetingState, MeetingStatus, MeetingSummary, ModelStatus,
    Segment, Settings, Speaker, Translation,
};
use tauri::{AppHandle, State};

use self::jobs::{begin_job, cancel_job, cancellable, is_cancelled, Job};
pub(crate) use self::processing::spawn_recovery;
pub(crate) use self::recording::on_system_audio_failed;
use crate::events;
use crate::features::{llm, models};
use crate::state::{lock, AppState};

/// The error a cancelled job rejects with.
const CANCELLED: &str = "Cancelled";
const MIC_FILE: &str = "mic.wav";
const SYSTEM_FILE: &str = "system.wav";
/// Unix ms of the system recording's first sample, kept so a retry can line it up again.
const SYSTEM_START_FILE: &str = "system-started-at";
const MIXED_FILE: &str = "mixed.wav";
const WINDOW_FILE: &str = "window.wav";
/// 16 kHz: samples per millisecond.
const SAMPLES_PER_MS: i64 = 16;

/// The meeting being recorded and the jobs running per meeting.
#[derive(Default)]
pub(crate) struct MeetingsState {
    recording: tokio::sync::Mutex<Option<recording::Active>>,
    jobs: Mutex<HashMap<i64, Job>>,
    next_job: AtomicU64,
}

impl fmt::Debug for MeetingsState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let jobs: Vec<(i64, MeetingJob)> = lock(&self.jobs)
            .iter()
            .map(|(id, job)| (*id, job.kind))
            .collect();
        f.debug_struct("MeetingsState")
            .field("jobs", &jobs)
            .finish_non_exhaustive()
    }
}

/// Runs blocking file work (WAV mixing and cutting) off the async runtime.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| AppError::Other(format!("A background task failed: {error}")))?
}

fn emit_state(
    app: &AppHandle,
    id: i64,
    status: MeetingStatus,
    elapsed_ms: i64,
    message: Option<String>,
) {
    let payload = MeetingState {
        id,
        status,
        elapsed_ms,
        message,
    };
    events::emit(app, events::MEETING_STATE, payload);
}

/// `meeting-state` for a meeting whose status just changed.
fn emit_meeting(app: &AppHandle, meeting: &Meeting) {
    emit_state(
        app,
        meeting.id,
        meeting.status,
        meeting.duration_ms,
        meeting.error.clone(),
    );
}

fn emit_progress(
    app: &AppHandle,
    id: i64,
    job: MeetingJob,
    fraction: Option<f32>,
    message: Option<String>,
) {
    let payload = MeetingProgress {
        id,
        job,
        fraction: fraction.map(|fraction| fraction.clamp(0.0, 1.0)),
        message,
    };
    events::emit(app, events::MEETING_PROGRESS, payload);
}

fn changed(app: &AppHandle) {
    events::emit(app, events::MEETINGS_CHANGED, ());
}

/// The model that transcribes meetings: the chosen local model, or the best ready model of
/// Smart Select's route (its top recommendation when none is ready, so readiness names it).
async fn transcription_model(state: &AppState, settings: &Settings) -> String {
    match models::active_model(state, settings).await {
        Some(model) => model,
        None => models::smart_select::local_model(&settings.languages).to_owned(),
    }
}

/// Why `model` cannot do its part of a meeting right now; `None` when it is ready.
async fn blocking_reason(
    state: &AppState,
    settings: &Settings,
    model: &str,
    purpose: &str,
) -> Option<String> {
    let info = models::model_status(state, settings, model).await;
    match info.status {
        ModelStatus::Ready => None,
        ModelStatus::NotDownloaded => Some(format!(
            "{} is not downloaded. Download it in Settings to {purpose}.",
            info.name
        )),
        ModelStatus::Downloading => Some(format!(
            "{} is still downloading. It is needed to {purpose}.",
            info.name
        )),
        ModelStatus::Unsupported => Some(
            info.reason
                .unwrap_or_else(|| format!("{} is not available on this Mac.", info.name)),
        ),
    }
}

async fn readiness(state: &AppState, settings: &Settings) -> MeetingReadiness {
    let transcription = blocking_reason(
        state,
        settings,
        &transcription_model(state, settings).await,
        "transcribe meetings",
    )
    .await;
    let diarization = blocking_reason(
        state,
        settings,
        DIARIZER,
        "tell the speakers of a meeting apart",
    )
    .await;
    let assistant = llm::backend_model(settings, settings.meeting_assistant);
    let language = llm::unavailable(state, settings, &assistant).await;
    let translation = blocking_reason(state, settings, TRANSLATOR, "translate meetings").await;
    MeetingReadiness {
        transcription,
        diarization,
        language,
        translation,
    }
}

/// How long the kept recording runs: the microphone file, or the system file shifted by its
/// later start when that ends later. Files that cannot be read count as empty.
async fn recorded_duration(dir: &Path, mic_started_at: i64) -> i64 {
    let mic = dir.join(MIC_FILE);
    let system = dir.join(SYSTEM_FILE);
    let offset = system_offset_ms(dir, mic_started_at).await;
    let measured = blocking(move || {
        let mic = sv_audio::wav_sample_count(&mic).unwrap_or(0);
        let system = if system.is_file() {
            sv_audio::wav_sample_count(&system).unwrap_or(0)
        } else {
            0
        };
        Ok((mic, system))
    })
    .await;
    let (mic, system) = measured.unwrap_or((0, 0));
    let system_end = if system > 0 {
        offset.saturating_add(sv_audio::samples_to_ms(system))
    } else {
        0
    };
    sv_audio::samples_to_ms(mic).max(system_end).max(0)
}

/// How much later than the microphone the system recording started, in ms; 0 when unknown.
async fn system_offset_ms(dir: &Path, mic_started_at: i64) -> i64 {
    match tokio::fs::read_to_string(dir.join(SYSTEM_START_FILE)).await {
        Ok(text) => match text.trim().parse::<i64>() {
            Ok(started) => started.saturating_sub(mic_started_at),
            Err(error) => {
                tracing::warn!(%error, "unreadable system audio start time; assuming none");
                0
            }
        },
        Err(_) => 0,
    }
}

/// Removes a meeting's audio folder, only ever inside the meetings directory.
async fn remove_audio_dir(dir: &Path) {
    let root = match sv_storage::paths::meetings_dir() {
        Ok(root) => root,
        Err(error) => {
            tracing::warn!(%error, "meetings folder unavailable; audio not removed");
            return;
        }
    };
    let inside = match (std::fs::canonicalize(&root), std::fs::canonicalize(dir)) {
        (Ok(root), Ok(dir)) => dir != root && dir.starts_with(&root),
        // Already gone.
        (_, Err(_)) => return,
        (Err(_), Ok(_)) => false,
    };
    if !inside {
        tracing::warn!(dir = %dir.display(), "refusing to remove audio outside the meetings folder");
        return;
    }
    if let Err(error) = tokio::fs::remove_dir_all(dir).await {
        tracing::warn!(%error, dir = %dir.display(), "could not remove meeting audio");
    }
}

#[tauri::command]
pub(crate) async fn list_meetings(state: State<'_, AppState>) -> AppResult<Vec<Meeting>> {
    state.db()?.meetings().await
}

#[tauri::command]
pub(crate) async fn get_meeting(state: State<'_, AppState>, id: i64) -> AppResult<MeetingDetail> {
    state.db()?.meeting_detail(id).await
}

#[tauri::command]
pub(crate) async fn meeting_readiness(state: State<'_, AppState>) -> AppResult<MeetingReadiness> {
    let settings = state.db()?.settings().await?;
    Ok(readiness(&state, &settings).await)
}

#[tauri::command]
pub(crate) async fn start_meeting(
    app: AppHandle,
    title: Option<String>,
    system_audio: bool,
) -> AppResult<Meeting> {
    recording::start(&app, title, system_audio).await
}

#[tauri::command]
pub(crate) async fn stop_meeting(app: AppHandle) -> AppResult<Meeting> {
    recording::stop(&app).await
}

#[tauri::command]
pub(crate) async fn cancel_meeting(app: AppHandle) -> AppResult<()> {
    recording::cancel(&app).await
}

#[tauri::command]
pub(crate) async fn delete_meeting(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    if recording::is_recording(&app, id).await {
        return Err(AppError::invalid(
            "This meeting is still recording. Stop or discard the recording first.",
        ));
    }
    cancel_job(&app, id);
    let dir = state.db()?.delete_meeting(id).await?;
    remove_audio_dir(Path::new(&dir)).await;
    changed(&app);
    Ok(())
}

#[tauri::command]
pub(crate) async fn rename_meeting(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    title: String,
) -> AppResult<Meeting> {
    let meeting = state.db()?.rename_meeting(id, &title).await?;
    changed(&app);
    Ok(meeting)
}

#[tauri::command]
pub(crate) async fn rename_speaker(
    app: AppHandle,
    state: State<'_, AppState>,
    meeting_id: i64,
    speaker_id: i64,
    name: Option<String>,
) -> AppResult<Speaker> {
    let name = name.unwrap_or_default();
    let speaker = state
        .db()?
        .rename_speaker(meeting_id, speaker_id, &name)
        .await?;
    changed(&app);
    Ok(speaker)
}

#[tauri::command]
pub(crate) async fn update_segment(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    text: String,
) -> AppResult<Segment> {
    let segment = state.db()?.update_meeting_segment(id, &text).await?;
    changed(&app);
    Ok(segment)
}

#[tauri::command]
pub(crate) async fn summarize_meeting(app: AppHandle, id: i64) -> AppResult<MeetingSummary> {
    assistant::summarize(&app, id).await
}

#[tauri::command]
pub(crate) async fn ask_meeting(
    app: AppHandle,
    id: i64,
    question: String,
) -> AppResult<ChatMessage> {
    assistant::ask(&app, id, &question).await
}

#[tauri::command]
pub(crate) async fn translate_meeting(
    app: AppHandle,
    id: i64,
    language: String,
) -> AppResult<Translation> {
    assistant::translate(&app, id, &language).await
}

#[tauri::command]
pub(crate) async fn cancel_meeting_job(app: AppHandle, id: i64) -> AppResult<()> {
    cancel_job(&app, id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn retry_meeting(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<Meeting> {
    let meeting = state.db()?.meeting(id).await?;
    if !matches!(
        meeting.status,
        MeetingStatus::Failed | MeetingStatus::Cancelled
    ) {
        return Err(AppError::invalid(
            "Only a meeting whose processing failed or was cancelled can be retried.",
        ));
    }
    processing::start(&app, id).await
}

#[tauri::command]
pub(crate) async fn export_meeting(
    state: State<'_, AppState>,
    id: i64,
    path: String,
    format: ExportFormat,
) -> AppResult<()> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err(AppError::invalid("Choose where to save the export."));
    }
    let detail = state.db()?.meeting_detail(id).await?;
    let started = chrono::Local
        .timestamp_millis_opt(detail.meeting.started_at)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_default();
    let content = match format {
        ExportFormat::Markdown => sv_text::meetings::export_markdown(&detail, &started),
        ExportFormat::Text => sv_text::meetings::export_text(&detail, &started),
        ExportFormat::Json => serde_json::to_string_pretty(&detail).map_err(AppError::other)?,
    };
    tokio::fs::write(&path, content)
        .await
        .map_err(|error| AppError::Io(format!("Could not write {}: {error}", path.display())))
}
