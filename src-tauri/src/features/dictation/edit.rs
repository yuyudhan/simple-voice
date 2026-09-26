// FilePath: src-tauri/src/features/dictation/edit.rs
//! Edit mode: the user selects text, holds the edit shortcut and says how to change it. The
//! selection is read the moment the shortcut goes down, the recording becomes the instruction,
//! the configured AI provider rewrites the selection, and the result is pasted over the
//! still-selected text in the same app, where Cmd+Z undoes it. A failed edit leaves the
//! selection untouched.

use std::time::{Duration, Instant};

use sv_audio::{encode_wav, Cue};
use sv_domain::{
    AppError, AppResult, DictationPhase, DictationState, HistoryStatus, NewHistory, PostProcessing,
    Settings,
};
use sv_engine::FrontmostApp;
use sv_storage::Db;
use sv_text::EDIT_MAX_CHARS;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};

use super::delivery::{self, Delivery};
use super::pipeline::SessionInput;
use super::transcription::{discard_audio, transcribe_fresh, vocabulary, Audio};
use super::{elapsed_ms, llm, publish};
use crate::events;
use crate::features::permissions;
use crate::state::AppState;

/// The selection read that starts with an edit recording; `None` when nothing is selected.
pub(crate) type SelectionTask = JoinHandle<AppResult<Option<String>>>;

/// Longest the pipeline waits for the selection read that started with the recording.
const SELECTION_WAIT: Duration = Duration::from_secs(3);
/// Longest the check for a focus change may delay the paste.
const FRONTMOST_TIMEOUT: Duration = Duration::from_millis(300);

const NO_SELECTION: &str = "Select the text to edit first";
const NO_PROVIDER: &str = "Edit mode needs AI post-processing — choose a provider in Settings → \
                           Models";
const APP_CHANGED: &str = "Edited text copied — the app changed before the edit finished";

/// Starts reading the selection in the focused app. Called when the edit shortcut goes down,
/// before the user can move focus or the selection.
pub(crate) fn capture_selection(app: &AppHandle) -> SelectionTask {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { app.state::<AppState>().engine.selected_text().await })
}

/// A state of an edit session, so the overlay can tell edits from dictations.
pub(crate) fn edit_state(phase: DictationPhase, id: u64) -> DictationState {
    let mut state = DictationState::new(phase, id);
    state.edit = true;
    state
}

pub(super) async fn run_edit(app: AppHandle, input: SessionInput, selection: SelectionTask) {
    let SessionInput {
        id,
        recording,
        settings,
        started_at_ms,
        stopped_at,
        frontmost,
        selection: _,
    } = input;
    let state = app.state::<AppState>();
    publish(&app, edit_state(DictationPhase::Transcribing, id));

    if settings.post_processing == PostProcessing::Off {
        return fail(&app, &settings, id, NO_PROVIDER);
    }
    let selection = match read_selection(&app, selection).await {
        Ok(selection) => selection,
        Err(message) => return fail(&app, &settings, id, message),
    };
    let db = match state.db() {
        Ok(db) => db.clone(),
        Err(error) => return fail(&app, &settings, id, error.to_string()),
    };

    let vocabulary = vocabulary(&state).await;
    let mut audio = Audio {
        wav: encode_wav(&recording.samples),
        path: None,
    };
    let heard = transcribe_fresh(&state, &settings, &vocabulary, id, &mut audio).await;
    // An edit is never retried from its audio: the selection it applied to is gone by then.
    discard_audio(&audio).await;
    let (app_name, bundle_id) = frontmost
        .clone()
        .map_or((None, None), |front| (front.name, front.bundle_id));
    let mut row = NewHistory {
        created_at: started_at_ms,
        status: HistoryStatus::Failed,
        raw_text: String::new(),
        final_text: String::new(),
        source_text: Some(selection.clone()),
        error: None,
        model: settings.transcription_model.clone(),
        format_model: None,
        language: None,
        style: settings.style,
        audio_ms: recording.duration_ms,
        latency_ms: 0,
        transcribe_ms: None,
        format_ms: None,
        dictionary_fixes: 0,
        app_name,
        bundle_id,
        audio_path: None,
    };
    let heard = match heard {
        Ok(heard) if heard.text.trim().is_empty() => {
            let mut cancelled = edit_state(DictationPhase::Cancelled, id);
            cancelled.message = Some("No speech detected".to_owned());
            return publish(&app, cancelled);
        }
        Ok(heard) => heard,
        Err(error) => {
            row.error = Some(error.to_string());
            row.latency_ms = elapsed_ms(stopped_at);
            save(&app, &db, row).await;
            return fail(&app, &settings, id, error.to_string());
        }
    };
    let instruction = sv_text::format(&heard.text, &vocabulary, settings.style);
    row.raw_text = heard.text;
    row.transcribe_ms = Some(heard.ms);
    row.language = heard.language;
    row.dictionary_fixes = i64::from(instruction.rule_hits);

    publish(&app, edit_state(DictationPhase::Formatting, id));
    let prompt = sv_text::edit_prompt(&selection, &instruction.text, &vocabulary.terms);
    let started = Instant::now();
    let limit = sv_text::edit_timeout(&selection);
    let max_tokens = sv_text::edit_max_tokens(&selection);
    let edited = match llm::complete(&state, &settings, &prompt, max_tokens, limit).await {
        Ok(reply) => sv_text::accept_edit(&selection, &reply.text, reply.finished),
        Err(reason) => Err(reason),
    };
    let edited = match edited {
        Ok(edited) => edited,
        Err(reason) => {
            tracing::info!(session = id, %reason, "edit rejected; selection left as it was");
            let message = format!("Edit failed: {reason}");
            row.error = Some(message.clone());
            row.latency_ms = elapsed_ms(stopped_at);
            save(&app, &db, row).await;
            return fail(&app, &settings, id, message);
        }
    };
    row.format_model = llm::model_id(&settings);
    row.format_ms = Some(elapsed_ms(started));

    let delivered = if still_in(&state, frontmost.as_ref()).await {
        delivery::deliver_in_order(&app, id, &edited, settings.restore_clipboard).await
    } else {
        match delivery::copy_to_clipboard(&edited) {
            Ok(()) => Delivery::NotPasted(APP_CHANGED.to_owned()),
            Err(error) => Delivery::NotPasted(format!("Could not copy the edited text: {error}")),
        }
    };
    row.latency_ms = elapsed_ms(stopped_at);
    row.final_text = edited;
    row.status = match &delivered {
        Delivery::Pasted => HistoryStatus::Pasted,
        Delivery::Dropped => HistoryStatus::Dropped,
        Delivery::NotPasted(message) => {
            row.error = Some(message.clone());
            HistoryStatus::NotPasted
        }
    };
    save(&app, &db, row).await;

    match delivered {
        Delivery::Pasted => publish(&app, edit_state(DictationPhase::Done, id)),
        Delivery::Dropped => {
            tracing::info!(
                session = id,
                "a newer dictation was already pasted; edit kept in history"
            );
        }
        Delivery::NotPasted(message) => fail(&app, &settings, id, message),
    }
}

/// Waits for the selection read that started with the recording and checks it is usable.
async fn read_selection(app: &AppHandle, task: SelectionTask) -> Result<String, String> {
    let read = match tokio::time::timeout(SELECTION_WAIT, task).await {
        Ok(Ok(read)) => read,
        Ok(Err(error)) => {
            tracing::error!(%error, "the selection read did not finish");
            return Err("Could not read the selected text".to_owned());
        }
        Err(_) => return Err("Reading the selected text timed out".to_owned()),
    };
    match read {
        Ok(Some(text)) if text.chars().count() > EDIT_MAX_CHARS => Err(format!(
            "Select at most {EDIT_MAX_CHARS} characters to edit"
        )),
        Ok(Some(text)) if !text.trim().is_empty() => Ok(text),
        Ok(_) => Err(NO_SELECTION.to_owned()),
        Err(AppError::Permission(message)) => {
            if let Err(error) = permissions::announce(app).await {
                tracing::debug!(%error, "permission refresh after a selection read failed");
            }
            Err(message)
        }
        Err(error) => Err(format!("Could not read the selected text: {error}")),
    }
}

/// Whether the app the edit started in is still frontmost, so the paste lands on its selection.
/// When either answer is unknown the paste goes ahead, as it does for a dictation.
async fn still_in(state: &AppState, started_in: Option<&FrontmostApp>) -> bool {
    let Some(expected) = started_in.and_then(|app| app.bundle_id.as_deref()) else {
        return true;
    };
    match tokio::time::timeout(FRONTMOST_TIMEOUT, state.engine.frontmost_app()).await {
        Ok(Ok(now)) => now.bundle_id.as_deref().is_none_or(|now| now == expected),
        _ => true,
    }
}

async fn save(app: &AppHandle, db: &Db, row: NewHistory) {
    if let Err(error) = db.insert_history(row).await {
        tracing::error!(%error, "could not save the edit to history");
    }
    events::history_changed(app);
}

fn fail(app: &AppHandle, settings: &Settings, id: u64, message: impl Into<String>) {
    app.state::<AppState>().play(settings, Cue::Error);
    let mut failed = edit_state(DictationPhase::Error, id);
    failed.message = Some(message.into());
    publish(app, failed);
}
