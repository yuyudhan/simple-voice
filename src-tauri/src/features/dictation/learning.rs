// FilePath: src-tauri/src/features/dictation/learning.rs
//! Learning from corrections: after a dictation is pasted, the engine helper watches the field
//! it went into; words the user then corrects are judged by the post-processing model, and the
//! ones it picks join the personal dictionary as learned words.

use std::time::Duration;

use sv_domain::{PostProcessing, Settings};
use tauri::{AppHandle, Manager};

use super::llm;
use crate::events;
use crate::state::AppState;

/// How long the helper follows the field after a paste. Corrections come right after reading
/// the result; a longer watch mostly records unrelated writing.
const WATCH_WINDOW: Duration = Duration::from_secs(60);

/// Learning needs the setting and a model to judge the corrections.
pub(super) fn enabled(settings: &Settings) -> bool {
    settings.learn_from_edits && settings.post_processing != PostProcessing::Off
}

/// Ends the previous watch before a new recording can change the field, and gives a Chromium
/// app time to build its accessibility tree while the user speaks.
pub(super) fn prepare(app: &AppHandle, settings: &Settings) {
    if !enabled(settings) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = app.state::<AppState>().engine.prepare_edit_watch().await {
            tracing::debug!(%error, "could not prepare the correction watch");
        }
    });
}

/// Follows the field `pasted` went into and learns from what the user corrects there.
pub(super) fn watch(app: &AppHandle, history_id: i64, pasted: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        learn(&app, history_id, &pasted).await;
    });
}

async fn learn(app: &AppHandle, history_id: i64, pasted: &str) {
    let state = app.state::<AppState>();
    let watch = match state.engine.watch_edits(pasted, WATCH_WINDOW).await {
        Ok(watch) => watch,
        Err(error) => {
            tracing::debug!(%error, "correction watch failed");
            return;
        }
    };
    let Some(edited) = watch.text else {
        tracing::debug!(reason = ?watch.reason, "no corrections read");
        return;
    };
    let edited = sv_text::without_invisible(&edited);
    let edited = edited.trim();
    if edited.is_empty() || edited == pasted.trim() {
        return;
    }
    let Ok(db) = state.db().cloned() else {
        return;
    };
    match db.set_history_edited_text(history_id, edited).await {
        Ok(()) => events::history_changed(app),
        Err(error) => tracing::warn!(%error, "could not record the corrected text"),
    }

    let corrections = sv_text::corrections(pasted, edited);
    if corrections.is_empty() {
        return;
    }
    // Read again: the user may have switched learning off during the watch, and nothing may
    // leave the machine after that.
    let settings = match db.settings().await {
        Ok(settings) if enabled(&settings) => settings,
        Ok(_) => return,
        Err(error) => {
            tracing::warn!(%error, "could not read settings for learning");
            return;
        }
    };
    let prompt = sv_text::learning_prompt(&corrections);
    let reply = llm::complete(
        &state,
        &settings,
        &prompt,
        sv_text::LEARNING_MAX_TOKENS,
        sv_text::LEARNING_TIMEOUT,
    )
    .await;
    let reply = match reply {
        Ok(reply) => reply,
        Err(reason) => {
            tracing::info!(reason, "could not judge the corrections");
            return;
        }
    };
    let words = sv_text::accept_learning(&corrections, &reply.text, reply.finished);
    if words.is_empty() {
        return;
    }
    match db.learn_words(&words).await {
        Ok(learned) if !learned.is_empty() => {
            tracing::info!(count = learned.len(), "learned words from corrections");
            events::dictionary_changed(app);
        }
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "could not save learned words"),
    }
}
