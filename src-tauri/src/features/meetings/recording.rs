// FilePath: src-tauri/src/features/meetings/recording.rs
//! Recording a meeting: the microphone streams straight into `mic.wav` from the capture worker
//! (nothing accumulates in memory, however long the meeting), and with the user's consent the
//! helper records system audio into `system.wav`. A ticker reports the elapsed time about once
//! a second.

use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sv_audio::{ChunkSink, Recorder, WavWriter};
use sv_domain::{AppError, AppResult, Meeting, MeetingStatus, PermissionStatus};
use tauri::{AppHandle, Manager};

use super::{
    changed, emit_meeting, emit_state, processing, readiness, recorded_duration, remove_audio_dir,
    MeetingsState, MIC_FILE, SYSTEM_FILE, SYSTEM_START_FILE,
};
use crate::features::permissions;
use crate::state::{lock, now_ms, AppState};

const TICK: Duration = Duration::from_secs(1);
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MICROPHONE_DENIED: &str = "Simple Voice needs microphone access to record meetings. Grant \
it in System Settings → Privacy & Security → Microphone.";
const SYSTEM_AUDIO_DENIED: &str = "Recording system audio needs the Screen & System Audio \
Recording permission. Allow Simple Voice in System Settings → Privacy & Security → Screen & \
System Audio Recording, or record the microphone only.";

/// Where the capture worker writes; the first write error ends writing and is reported at stop.
#[derive(Debug)]
struct MicFile {
    writer: Option<WavWriter>,
    error: Option<String>,
}

/// The meeting being recorded.
pub(super) struct Active {
    id: i64,
    /// Unix ms the microphone started; the meeting's `started_at`.
    started_at_ms: i64,
    dir: PathBuf,
    recorder: Recorder,
    mic: Arc<Mutex<MicFile>>,
    system_audio: bool,
    /// Shown with every elapsed-time update once system audio stopped by itself.
    notice: Arc<Mutex<Option<String>>>,
    ticker: tauri::async_runtime::JoinHandle<()>,
}

pub(super) async fn is_recording(app: &AppHandle, id: i64) -> bool {
    let meetings = app.state::<MeetingsState>();
    let slot = meetings.recording.lock().await;
    slot.as_ref().is_some_and(|active| active.id == id)
}

pub(super) async fn start(
    app: &AppHandle,
    title: Option<String>,
    system_audio: bool,
) -> AppResult<Meeting> {
    let meetings = app.state::<MeetingsState>();
    // Held until the recording is in place, so two starts can never both begin.
    let mut slot = meetings.recording.lock().await;
    if slot.is_some() {
        return Err(AppError::invalid("A meeting is already being recorded."));
    }
    let state = app.state::<AppState>();
    let settings = state.db()?.settings().await?;
    let ready = readiness(&state, &settings).await;
    if let Some(reason) = ready.transcription.or(ready.diarization) {
        return Err(AppError::Other(reason));
    }
    let granted = tokio::time::timeout(PROBE_TIMEOUT, state.engine.permissions())
        .await
        .map_err(|_| AppError::Engine("The on-device engine did not respond.".to_owned()))??;
    permissions::observe(app, granted);
    if matches!(
        granted.microphone,
        PermissionStatus::Denied | PermissionStatus::Restricted
    ) {
        return Err(AppError::Permission(MICROPHONE_DENIED.to_owned()));
    }
    if system_audio && granted.system_audio != PermissionStatus::Granted {
        return Err(AppError::Permission(SYSTEM_AUDIO_DENIED.to_owned()));
    }

    let title = title
        .map(|title| title.trim().to_owned())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(default_title);
    let dir = sv_storage::paths::meetings_dir()?.join(format!("meeting-{}", now_ms()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&dir)
        .map_err(|error| AppError::Io(format!("Could not create {}: {error}", dir.display())))?;
    let active = match begin(app, &state, &dir, &title, system_audio, settings.microphone).await {
        Ok(active) => active,
        Err(error) => {
            remove_audio_dir(&dir).await;
            return Err(error);
        }
    };
    let id = active.0.id;
    *slot = Some(active.0);
    drop(slot);
    tracing::info!(meeting = id, system_audio, "meeting recording started");
    emit_state(app, id, MeetingStatus::Recording, 0, None);
    changed(app);
    Ok(active.1)
}

fn default_title() -> String {
    format!("Meeting {}", chrono::Local::now().format("%b %-d, %H:%M"))
}

/// Starts capture into `dir` and creates the meeting row; undoes the capture on failure.
async fn begin(
    app: &AppHandle,
    state: &AppState,
    dir: &Path,
    title: &str,
    system_audio: bool,
    microphone: Option<String>,
) -> AppResult<(Active, Meeting)> {
    let writer = WavWriter::create(&dir.join(MIC_FILE))?;
    let mic = Arc::new(Mutex::new(MicFile {
        writer: Some(writer),
        error: None,
    }));
    let sink_file = Arc::clone(&mic);
    let sink: ChunkSink = Box::new(move |chunk: &[i16]| {
        let mut file = lock(&sink_file);
        if let Some(writer) = file.writer.as_mut() {
            if let Err(error) = writer.append(chunk) {
                file.error = Some(error.to_string());
                file.writer = None;
            }
        }
    });
    let recorder = tauri::async_runtime::spawn_blocking(move || {
        Recorder::start_with(
            microphone.as_deref(),
            Box::new(|_level: f32| {}),
            Some(sink),
            false,
        )
    })
    .await
    .map_err(|error| AppError::Audio(format!("Could not start the microphone: {error}")))?
    .map_err(|error| AppError::Audio(format!("Could not start the microphone: {error}")))?;
    let started_at_ms = now_ms();

    if system_audio {
        if let Err(error) = start_system_audio(state, dir).await {
            discard(recorder).await;
            return Err(error);
        }
    }
    let Some(dir_text) = dir.to_str() else {
        stop_system_audio(state, system_audio).await;
        discard(recorder).await;
        return Err(AppError::invalid(
            "The meetings folder path must be valid UTF-8.",
        ));
    };
    let created = match state.db() {
        Ok(db) => {
            db.create_meeting(title, started_at_ms, true, system_audio, dir_text)
                .await
        }
        Err(error) => Err(error),
    };
    let meeting = match created {
        Ok(meeting) => meeting,
        Err(error) => {
            stop_system_audio(state, system_audio).await;
            discard(recorder).await;
            return Err(error);
        }
    };

    let notice = Arc::new(Mutex::new(None));
    let ticker = {
        let app = app.clone();
        let notice = Arc::clone(&notice);
        let id = meeting.id;
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(TICK).await;
                let message = lock(&notice).clone();
                emit_state(
                    &app,
                    id,
                    MeetingStatus::Recording,
                    now_ms() - started_at_ms,
                    message,
                );
            }
        })
    };
    let active = Active {
        id: meeting.id,
        started_at_ms,
        dir: dir.to_path_buf(),
        recorder,
        mic,
        system_audio,
        notice,
        ticker,
    };
    Ok((active, meeting))
}

/// Starts the helper's system audio capture and remembers when its first sample was taken.
async fn start_system_audio(state: &AppState, dir: &Path) -> AppResult<()> {
    let started = state
        .engine
        .system_audio_start(&dir.join(SYSTEM_FILE))
        .await?;
    let written = tokio::fs::write(
        dir.join(SYSTEM_START_FILE),
        started.started_at_ms.to_string(),
    )
    .await;
    if let Err(error) = written {
        stop_system_audio(state, true).await;
        return Err(AppError::Io(format!(
            "Could not save the system audio start time: {error}"
        )));
    }
    Ok(())
}

async fn stop_system_audio(state: &AppState, running: bool) {
    if !running {
        return;
    }
    if let Err(error) = state.engine.system_audio_stop().await {
        tracing::warn!(%error, "system audio stop failed");
    }
}

/// Stops capture and throws its audio away.
async fn discard(recorder: Recorder) {
    let stopped = tauri::async_runtime::spawn_blocking(move || recorder.stop()).await;
    if let Ok(Err(error)) = stopped {
        tracing::warn!(%error, "discarded meeting capture ended with an error");
    }
}

/// Ends the recording, finalizes its files, and starts processing.
pub(super) async fn stop(app: &AppHandle) -> AppResult<Meeting> {
    let meetings = app.state::<MeetingsState>();
    let active = meetings
        .recording
        .lock()
        .await
        .take()
        .ok_or_else(|| AppError::invalid("No meeting is being recorded."))?;
    let state = app.state::<AppState>();
    let Active {
        id,
        started_at_ms,
        dir,
        recorder,
        mic,
        system_audio,
        ticker,
        ..
    } = active;
    ticker.abort();
    stop_system_audio(&state, system_audio).await;

    let mut problems: Vec<String> = Vec::new();
    match tauri::async_runtime::spawn_blocking(move || recorder.stop()).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => problems.push(error.to_string()),
        Err(error) => problems.push(error.to_string()),
    }
    let (writer, write_error) = {
        let mut file = lock(&mic);
        (file.writer.take(), file.error.take())
    };
    problems.extend(write_error);
    if let Some(writer) = writer {
        match super::blocking(move || writer.finish()).await {
            Ok(_) => {}
            Err(error) => problems.push(error.to_string()),
        }
    }
    for problem in &problems {
        tracing::warn!(meeting = id, %problem, "meeting microphone recording problem");
    }

    let duration_ms = recorded_duration(&dir, started_at_ms).await;
    let db = state.db()?;
    db.finish_meeting_recording(id, now_ms(), duration_ms)
        .await?;
    tracing::info!(meeting = id, duration_ms, "meeting recording stopped");
    if duration_ms == 0 {
        let reason = match problems.first() {
            Some(problem) => format!("Nothing was recorded: {problem}"),
            None => "Nothing was recorded.".to_owned(),
        };
        let meeting = db
            .set_meeting_status(id, MeetingStatus::Failed, Some(&reason))
            .await?;
        emit_meeting(app, &meeting);
        changed(app);
        return Ok(meeting);
    }
    processing::start(app, id).await
}

/// Discards the recording in progress: its audio and its row.
pub(super) async fn cancel(app: &AppHandle) -> AppResult<()> {
    let meetings = app.state::<MeetingsState>();
    let active = meetings
        .recording
        .lock()
        .await
        .take()
        .ok_or_else(|| AppError::invalid("No meeting is being recorded."))?;
    let state = app.state::<AppState>();
    let Active {
        id,
        started_at_ms,
        dir,
        recorder,
        system_audio,
        ticker,
        ..
    } = active;
    ticker.abort();
    stop_system_audio(&state, system_audio).await;
    discard(recorder).await;
    let deleted = state.db()?.delete_meeting(id).await;
    remove_audio_dir(&dir).await;
    deleted?;
    tracing::info!(meeting = id, "meeting recording discarded");
    emit_state(
        app,
        id,
        MeetingStatus::Cancelled,
        now_ms() - started_at_ms,
        None,
    );
    changed(app);
    Ok(())
}

/// The helper's system audio capture stopped by itself. The microphone keeps recording; the
/// meeting carries a notice saying so, and the audio captured until then is kept.
pub(crate) fn on_system_audio_failed(app: &AppHandle, reason: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let meetings = app.state::<MeetingsState>();
        let (id, started_at_ms, notice) = {
            let slot = meetings.recording.lock().await;
            match slot.as_ref() {
                Some(active) if active.system_audio => {
                    (active.id, active.started_at_ms, Arc::clone(&active.notice))
                }
                _ => return,
            }
        };
        tracing::warn!(meeting = id, %reason, "system audio capture stopped");
        let message = format!(
            "System audio stopped recording ({reason}). The microphone is still recording."
        );
        *lock(&notice) = Some(message.clone());
        let state = app.state::<AppState>();
        match state.db() {
            Ok(db) => {
                if let Err(error) = db.set_meeting_error(id, &message).await {
                    tracing::warn!(%error, meeting = id, "could not record the system audio notice");
                }
            }
            Err(error) => tracing::warn!(%error, "database unavailable"),
        }
        emit_state(
            &app,
            id,
            MeetingStatus::Recording,
            now_ms() - started_at_ms,
            Some(message),
        );
        changed(&app);
    });
}
