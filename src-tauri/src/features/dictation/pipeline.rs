// FilePath: src-tauri/src/features/dictation/pipeline.rs
//! One finished recording → text in the frontmost app and a history row. Transcription is an
//! on-device model run on the whole recording (a live stream only displayed it), then deterministic
//! formatting, then personal snippets (protected from what follows), then the optional on-device
//! post-processing pass, then ordered delivery. The same steps (minus pasting) retry a failed
//! entry. A recording made with the edit shortcut goes to edit mode instead.

use std::path::PathBuf;
use std::time::Instant;

use serde::Serialize;
use sv_audio::{encode_wav, Cue, Recording};
use sv_domain::{
    AppError, AppResult, DictationPhase, DictationState, HistoryEntry, HistoryStatus, NewHistory,
    PostProcessing, Settings, Snippet, StyleSource,
};
use sv_engine::FrontmostApp;
use sv_text::{PolishOutcome, Vocabulary};
use tauri::{AppHandle, Manager};

use super::context::{self, ContextTask};
use super::delivery::{self, Delivery};
use super::edit::{self, SelectionTask};
use super::live::LiveSession;
use super::transcription::{
    discard_audio, keep_audio, transcribe, transcribe_fresh, vocabulary, Audio, Unheard,
};
use super::{elapsed_ms, learning, publish, publish_message, publish_phase};
use crate::events;
use crate::features::history::remove_audio;
use crate::features::{llm, snippets};
use crate::state::AppState;

const UNFORMATTED: &str = "unformatted";
/// Formatting only shortens a transcript, so its reply fits comfortably.
const POLISH_MAX_TOKENS: u32 = 2048;
const TEST_SAMPLE: &str = "um so i think we should uh move the standup to ten tomorrow and \
     the the api review to friday and can you send the notes to priya";

pub(crate) struct SessionInput {
    pub(crate) id: u64,
    pub(crate) recording: Recording,
    pub(crate) settings: Settings,
    pub(crate) started_at_ms: i64,
    pub(crate) stopped_at: Instant,
    pub(crate) frontmost: Option<FrontmostApp>,
    /// Set for an edit: the selection read when the edit shortcut went down.
    pub(crate) selection: Option<SelectionTask>,
    /// Where `settings.style` came from (resolved for the frontmost app at the start).
    pub(crate) style_source: StyleSource,
    /// Spelling hints read from the focused field at the start, when context is on.
    pub(crate) context: Option<ContextTask>,
    /// The recording's live transcription stream, when the voice model streams.
    pub(crate) live: Option<LiveSession>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PostProcessingTest {
    output: String,
    latency_ms: i64,
}

/// Result of the post-processing pass. Every variant carries the text to paste, with personal
/// snippets expanded.
enum Polish {
    /// Formatted text and how long the pass took.
    Polished(String, i64),
    /// Why the pass failed or was rejected by the guards, and the deterministic text used
    /// instead.
    Unformatted(String, String),
    /// Off, or too short to be worth it: the deterministic text.
    NotRun(String),
}

pub(crate) async fn run_session(app: AppHandle, mut input: SessionInput) {
    if let Some(selection) = input.selection.take() {
        return edit::run_edit(app, input, selection).await;
    }
    let SessionInput {
        id,
        recording,
        settings,
        started_at_ms,
        stopped_at,
        frontmost,
        selection: _,
        style_source,
        context,
        live,
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
    let mut vocabulary = vocabulary(&state, &settings).await;
    vocabulary.add_context_terms(&context::collect(context).await);
    let snippets = snippets::load(&state).await;
    let (app_name, bundle_id) =
        frontmost.map_or((None, None), |front| (front.name, front.bundle_id));
    let mut row = NewHistory {
        created_at: started_at_ms,
        status: HistoryStatus::Failed,
        raw_text: String::new(),
        final_text: String::new(),
        source_text: None,
        error: None,
        model: settings.transcription_model.clone(),
        first_model: None,
        retry_reason: None,
        format_model: None,
        language: None,
        style: settings.style,
        style_source: Some(style_source),
        audio_ms: recording.duration_ms,
        latency_ms: 0,
        transcribe_ms: None,
        format_ms: None,
        dictionary_fixes: 0,
        app_name,
        bundle_id,
        audio_path: None,
    };

    let mut audio = Audio {
        wav: encode_wav(&recording.samples),
        path: None,
    };
    // The live stream only showed the words while recording; the whole recording is
    // transcribed for the pasted text.
    if let Some(live) = live {
        live.end(&app).await;
    }
    let heard = transcribe_fresh(&state, &settings, &vocabulary, id, &mut audio).await;
    let heard = match heard {
        Ok(heard) if sv_text::only_hesitations(&heard.text) => {
            discard_audio(&audio).await;
            return publish_message(&app, id, DictationPhase::Cancelled, "No speech detected");
        }
        Ok(heard) => heard,
        Err(unheard) => {
            // Never lose a dictation: keep the audio and a retryable history row.
            let error = unheard.error;
            let kept = keep_audio(&audio, id).await;
            row.model = unheard.model;
            row.first_model = unheard.first_model;
            row.retry_reason = unheard.retry_reason;
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
    let polish = finish_text(&state, &settings, &vocabulary, &snippets, &formatted.text).await;
    let (final_text, mut status, note) = match polish {
        Polish::Polished(text, ms) => {
            row.format_model = llm::post_processing_model(&settings).map(str::to_owned);
            row.format_ms = Some(ms);
            (text, HistoryStatus::Pasted, None)
        }
        Polish::NotRun(text) => (text, HistoryStatus::Pasted, None),
        Polish::Unformatted(reason, text) => {
            tracing::info!(reason, "post-processing skipped; pasting formatted text");
            // The History badge's tooltip shows this; a paste failure below overrides it.
            row.error = Some(reason);
            (
                text,
                HistoryStatus::Unformatted,
                Some(UNFORMATTED.to_owned()),
            )
        }
    };

    let pasted = delivery::with_separator(&final_text);
    let delivered = delivery::deliver_in_order(&app, id, &pasted, settings.restore_clipboard).await;
    row.latency_ms = elapsed_ms(stopped_at);
    row.raw_text = heard.text;
    row.model = heard.model;
    row.first_model = heard.first_model;
    row.retry_reason = heard.retry_reason;
    row.transcribe_ms = Some(heard.ms);
    row.language = heard.language;
    row.dictionary_fixes = i64::from(formatted.rule_hits) + i64::from(heard.dictionary_fixes);
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
    let shown = sv_text::preview(&row.final_text);
    let watch_edits = delivered == Delivery::Pasted && learning::enabled(&settings);
    match db.insert_history(row).await {
        Ok(entry) if watch_edits => learning::watch(&app, entry.id, pasted),
        Ok(_) => {}
        Err(error) => tracing::error!(%error, "could not save the dictation to history"),
    }
    discard_audio(&audio).await;
    events::history_changed(&app);

    match delivered {
        Delivery::Pasted => {
            let mut done = DictationState::new(DictationPhase::Done, id);
            done.text = Some(shown);
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
    let mut settings = db.settings().await?;
    // The style the app it was meant for takes now.
    let (style, style_source) =
        settings.style_for(entry.bundle_id.as_deref(), entry.app_name.as_deref());
    settings.style = style;
    let vocabulary = vocabulary(&state, &settings).await;
    let snippets = snippets::load(&state).await;
    let started = Instant::now();
    let mut row = NewHistory {
        created_at: entry.created_at,
        status: HistoryStatus::Failed,
        raw_text: entry.raw_text,
        final_text: entry.text,
        source_text: None,
        error: None,
        model: settings.transcription_model.clone(),
        first_model: None,
        retry_reason: None,
        format_model: None,
        language: entry.language,
        style: settings.style,
        style_source: Some(style_source),
        audio_ms: entry.audio_ms,
        latency_ms: entry.latency_ms,
        transcribe_ms: None,
        format_ms: None,
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
        Ok(heard) if sv_text::only_hesitations(&heard.text) => Err(Unheard {
            error: AppError::other("No speech was detected in the saved audio"),
            model: heard.model,
            first_model: heard.first_model,
            retry_reason: heard.retry_reason,
        }),
        other => other,
    };
    let heard = match heard {
        Ok(heard) => heard,
        Err(unheard) => {
            row.model = unheard.model;
            row.first_model = unheard.first_model;
            row.retry_reason = unheard.retry_reason;
            row.error = Some(unheard.error.to_string());
            db.update_history(id, row).await?;
            events::history_changed(app);
            return Err(unheard.error);
        }
    };

    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    let polish = finish_text(&state, &settings, &vocabulary, &snippets, &formatted.text).await;
    let final_text = match polish {
        Polish::Polished(text, ms) => {
            row.format_model = llm::post_processing_model(&settings).map(str::to_owned);
            row.format_ms = Some(ms);
            text
        }
        Polish::NotRun(text) | Polish::Unformatted(_, text) => text,
    };
    // A retry only copies: the app the dictation was meant for is no longer focused.
    if let Err(error) = delivery::copy_to_clipboard(&final_text) {
        tracing::warn!(%error, "could not copy the retried text");
        row.error = Some(format!("Could not copy to the clipboard: {error}"));
    }
    row.status = HistoryStatus::NotPasted;
    row.raw_text = heard.text;
    row.model = heard.model;
    row.first_model = heard.first_model;
    row.retry_reason = heard.retry_reason;
    row.transcribe_ms = Some(heard.ms);
    row.final_text = final_text;
    row.language = heard.language;
    row.dictionary_fixes = i64::from(formatted.rule_hits) + i64::from(heard.dictionary_fixes);
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
    let vocabulary = vocabulary(&state, &settings).await;
    let formatted = sv_text::format(TEST_SAMPLE, &vocabulary, settings.style);
    let started = Instant::now();
    let output = match finish_text(&state, &settings, &vocabulary, &[], &formatted.text).await {
        Polish::Polished(text, _) | Polish::NotRun(text) => text,
        Polish::Unformatted(reason, _) => {
            return Err(AppError::other(format!("Post-processing failed: {reason}")));
        }
    };
    Ok(PostProcessingTest {
        output,
        latency_ms: elapsed_ms(started),
    })
}

fn will_post_process(settings: &Settings, text: &str) -> bool {
    settings.post_processing != PostProcessing::Off
        && !text.trim().is_empty()
        && sv_text::should_skip_polish(text).is_none()
}

/// Expands personal snippets and runs the post-processing pass on `text` (the deterministic
/// formatter's output). Snippet payloads stand in the prompt as markers the model must keep,
/// so the pass can never rewrite them; a reply that loses or alters a marker is rejected and the
/// deterministic text, with its snippets expanded, is used.
async fn finish_text(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    snippets: &[Snippet],
    text: &str,
) -> Polish {
    if !will_post_process(settings, text) {
        return Polish::NotRun(sv_text::snippets::expand(text, snippets));
    }
    let protected = sv_text::snippets::protect(text, snippets);
    let mut prompt = sv_text::polish_prompt(
        protected.text(),
        &vocabulary.terms,
        settings.style,
        llm::post_processing_target(settings),
        &settings.languages,
    );
    if protected.hits() > 0 {
        prompt.system.push_str(sv_text::snippets::MARKER_RULE);
    }
    let started = Instant::now();
    let limit = sv_text::polish_timeout(protected.text());
    let reply = llm::post_process(state, settings, &prompt, POLISH_MAX_TOKENS, limit).await;
    let outcome = match reply {
        Ok(reply) => sv_text::accept_polish(protected.text(), &reply.text, reply.finished),
        Err(reason) => PolishOutcome::Skipped(reason),
    };
    let expanded = || protected.expanded().to_owned();
    match outcome {
        PolishOutcome::Polished(polished) => match protected.restore(&polished) {
            Some(restored) => Polish::Polished(restored, elapsed_ms(started)),
            None => Polish::Unformatted(
                "the formatting pass changed a snippet".to_owned(),
                expanded(),
            ),
        },
        PolishOutcome::Skipped(reason) => Polish::Unformatted(reason, expanded()),
    }
}
