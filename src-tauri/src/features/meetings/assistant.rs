// FilePath: src-tauri/src/features/meetings/assistant.rs
//! The meeting assistant: summary with action items and transcript-grounded answers, written by
//! the language model the user picked for meetings (on-device, or Groq with their own key, which
//! falls back to the local model when a request fails), and translations by the dedicated
//! on-device translation model (general chat models swapped weekdays and names). Long
//! transcripts are never sent whole: summaries map over bounded chunks and then reduce, answers
//! read only retrieved excerpts, translations go in bounded batches. Every reply is parsed
//! strictly; one that does not parse fails the job instead of being stored.

use std::time::Duration;

use sv_domain::languages::{self, Language};
use sv_domain::models::TRANSLATOR;
use sv_domain::{
    AppError, AppResult, ChatMessage, ChatRole, MeetingDetail, MeetingJob, MeetingStatus,
    MeetingSummary, Settings, TranslatedSegment, Translation,
};
use sv_text::meetings::{
    chat_excerpt_chars, chat_prompt, join_bounded, parse_chat_answer, parse_summary,
    render_summary, retrieve_excerpts, summary_chunk_chars, summary_chunks, summary_merge_prompt,
    summary_prompt, transcript_lines, translation_batches, ChatAnswer, ParsedSummary,
    CHAT_MAX_TOKENS, SUMMARY_MAX_TOKENS,
};
use sv_text::{PolishPrompt, PolishTarget};
use tauri::{AppHandle, Manager};

use super::{begin_job, cancellable, changed, emit_progress};
use crate::features::llm;
use crate::state::AppState;

const SUMMARY_TIMEOUT: Duration = Duration::from_secs(240);
const CHAT_TIMEOUT: Duration = Duration::from_secs(120);
/// One translation batch: a 3B model on the Mac translates a few thousand characters well
/// within this, including its first load.
const TRANSLATION_TIMEOUT: Duration = Duration::from_secs(240);
/// Characters of transcript per translation request.
const TRANSLATION_BATCH_CHARS: usize = 2_000;
const MAX_QUESTION_CHARS: usize = 1_000;

/// The model writing for meetings, the prompt family it needs, and the settings its Groq
/// fallback is chosen from.
#[derive(Debug)]
struct Assistant {
    model: String,
    target: PolishTarget,
    settings: Settings,
}

/// Loads a processed meeting and checks the assistant model can run.
async fn prepare(app: &AppHandle, id: i64) -> AppResult<(MeetingDetail, Assistant)> {
    let state = app.state::<AppState>();
    let db = state.db()?;
    let detail = db.meeting_detail(id).await?;
    if detail.meeting.status != MeetingStatus::Ready || detail.segments.is_empty() {
        return Err(AppError::invalid("This meeting has no transcript yet."));
    }
    let settings = db.settings().await?;
    let model = llm::backend_model(&settings, settings.meeting_assistant);
    if let Some(reason) = llm::unavailable(&state, &settings, &model).await {
        return Err(AppError::Other(reason));
    }
    let assistant = Assistant {
        target: llm::target(&model),
        model,
        settings,
    };
    Ok((detail, assistant))
}

/// Where one model request sits in a job of `steps` requests, for `meeting-progress`.
#[derive(Debug, Clone, Copy)]
struct Step {
    id: i64,
    job: MeetingJob,
    done: usize,
    steps: usize,
}

/// One complete reply from the assistant model; a reply cut off at the token limit is an
/// error, never a partial result. The helper's progress within the request moves the job's
/// fraction through this step's share, so a long request never sits at the step's start.
async fn reply(
    app: &AppHandle,
    assistant: &Assistant,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
    what: &str,
    step: Step,
) -> AppResult<String> {
    let state = app.state::<AppState>();
    let on_progress: sv_engine::ProgressCallback = {
        let app = app.clone();
        Box::new(move |fraction: f32, _message: Option<String>| {
            let within = fraction.clamp(0.0, 1.0);
            let overall = (step.done as f32 + within) / step.steps.max(1) as f32;
            emit_progress(&app, step.id, step.job, Some(overall.min(0.99)), None);
        })
    };
    let answer = llm::generate_with_fallback(
        &state,
        &assistant.settings,
        &assistant.model,
        prompt,
        max_tokens,
        limit,
        on_progress,
    )
    .await
    .map_err(|error| AppError::Other(format!("The {what} could not be written: {error}")))?;
    let reply = answer.reply;
    if !reply.finished {
        return Err(AppError::Other(format!(
            "The {what} was cut off before it finished. Try again, or switch the meeting \
             assistant in Settings."
        )));
    }
    Ok(reply.text)
}

fn parsed<T>(result: Result<T, String>, what: &str) -> AppResult<T> {
    result.map_err(|reason| AppError::Other(format!("The {what} was not usable: {reason}")))
}

pub(super) async fn summarize(app: &AppHandle, id: i64) -> AppResult<MeetingSummary> {
    let (detail, assistant) = prepare(app, id).await?;
    let (guard, cancelled) = begin_job(app, id, MeetingJob::Summarize)?;
    let written = cancellable(cancelled, write_summary(app, id, &detail, &assistant)).await;
    drop(guard);
    let summary = written?;
    let state = app.state::<AppState>();
    let saved = state
        .db()?
        .save_meeting_summary(id, &summary.text, &summary.action_items, &assistant.model)
        .await?;
    emit_progress(app, id, MeetingJob::Summarize, Some(1.0), None);
    changed(app);
    Ok(saved)
}

/// Summarizes each bounded chunk of the transcript, then merges the part summaries in bounded
/// groups until one remains.
async fn write_summary(
    app: &AppHandle,
    id: i64,
    detail: &MeetingDetail,
    assistant: &Assistant,
) -> AppResult<ParsedSummary> {
    let budget = summary_chunk_chars(assistant.target);
    let lines = transcript_lines(detail);
    let chunks = summary_chunks(&lines, budget);
    // Merging n part summaries takes at most n - 1 more requests.
    let steps = (chunks.len() * 2).saturating_sub(1).max(1);
    let mut done = 0;
    let mut partials = Vec::with_capacity(chunks.len());
    for (index, chunk) in chunks.iter().enumerate() {
        progress(app, id, MeetingJob::Summarize, done, steps);
        let prompt = summary_prompt(chunk, index + 1, chunks.len());
        let text = reply(
            app,
            assistant,
            &prompt,
            SUMMARY_MAX_TOKENS,
            SUMMARY_TIMEOUT,
            "summary",
            Step {
                id,
                job: MeetingJob::Summarize,
                done,
                steps,
            },
        )
        .await?;
        partials.push(parsed(parse_summary(&text), "summary")?);
        done += 1;
    }
    while partials.len() > 1 {
        let rendered: Vec<String> = partials.iter().map(render_summary).collect();
        let mut groups = join_bounded(&rendered, "\n\n", budget);
        if groups.len() >= rendered.len() {
            // Every part summary fills the budget alone: merge them two at a time.
            groups = rendered.chunks(2).map(|pair| pair.join("\n\n")).collect();
        }
        let mut merged = Vec::with_capacity(groups.len());
        for group in &groups {
            progress(app, id, MeetingJob::Summarize, done, steps);
            let prompt = summary_merge_prompt(group);
            let text = reply(
                app,
                assistant,
                &prompt,
                SUMMARY_MAX_TOKENS,
                SUMMARY_TIMEOUT,
                "summary",
                Step {
                    id,
                    job: MeetingJob::Summarize,
                    done,
                    steps,
                },
            )
            .await?;
            merged.push(parsed(parse_summary(&text), "summary")?);
            done += 1;
        }
        partials = merged;
    }
    partials
        .into_iter()
        .next()
        .ok_or_else(|| AppError::invalid("This meeting has no transcript to summarize."))
}

fn progress(app: &AppHandle, id: i64, job: MeetingJob, done: usize, steps: usize) {
    let fraction = done as f32 / steps.max(1) as f32;
    emit_progress(app, id, job, Some(fraction.min(0.99)), None);
}

/// Answers a question from the transcript; stores the question and the answer.
pub(super) async fn ask(app: &AppHandle, id: i64, question: &str) -> AppResult<ChatMessage> {
    let question = question.trim();
    if question.is_empty() {
        return Err(AppError::invalid("Type a question about the meeting."));
    }
    if question.chars().count() > MAX_QUESTION_CHARS {
        return Err(AppError::invalid(format!(
            "Questions can be at most {MAX_QUESTION_CHARS} characters."
        )));
    }
    let (detail, assistant) = prepare(app, id).await?;
    let (guard, cancelled) = begin_job(app, id, MeetingJob::Chat)?;
    let answered = cancellable(cancelled, answer(app, id, &detail, &assistant, question)).await;
    drop(guard);
    let answer = answered?;
    let state = app.state::<AppState>();
    let db = state.db()?;
    db.add_meeting_chat(id, ChatRole::User, question, &[])
        .await?;
    let message = db
        .add_meeting_chat(id, ChatRole::Assistant, &answer.text, &answer.citations)
        .await?;
    emit_progress(app, id, MeetingJob::Chat, Some(1.0), None);
    changed(app);
    Ok(message)
}

async fn answer(
    app: &AppHandle,
    id: i64,
    detail: &MeetingDetail,
    assistant: &Assistant,
    question: &str,
) -> AppResult<ChatAnswer> {
    emit_progress(app, id, MeetingJob::Chat, None, None);
    let lines = transcript_lines(detail);
    let excerpts = retrieve_excerpts(&lines, question, chat_excerpt_chars(assistant.target));
    let prompt = chat_prompt(&excerpts, question);
    let text = reply(
        app,
        assistant,
        &prompt,
        CHAT_MAX_TOKENS,
        CHAT_TIMEOUT,
        "answer",
        Step {
            id,
            job: MeetingJob::Chat,
            done: 0,
            steps: 1,
        },
    )
    .await?;
    parsed(parse_chat_answer(&text, &excerpts), "answer")
}

/// Translates the whole transcript into `language` (a registry tag) with the dedicated
/// translation model and stores it, replacing an earlier translation into the same language.
pub(super) async fn translate(app: &AppHandle, id: i64, language: &str) -> AppResult<Translation> {
    let target = languages::language(language)
        .ok_or_else(|| AppError::invalid(format!("“{language}” is not a supported language.")))?;
    if target.is_variant() {
        // The model writes each language in its own script only; a romanised variant would
        // silently come back as the base language.
        return Err(AppError::invalid(format!(
            "Translation into {} is not supported: the translation model writes {} only in {}.",
            target.name,
            target.base,
            target.native.user_name()
        )));
    }
    let state = app.state::<AppState>();
    let settings = state.db()?.settings().await?;
    if let Some(reason) = super::readiness(&state, &settings).await.translation {
        return Err(AppError::Other(reason));
    }
    let detail = state.db()?.meeting_detail(id).await?;
    if detail.segments.is_empty() {
        return Err(AppError::invalid(
            "This meeting has no transcript to translate.",
        ));
    }
    let (guard, cancelled) = begin_job(app, id, MeetingJob::Translate)?;
    let translated = cancellable(cancelled, translate_segments(app, id, &detail, target)).await;
    drop(guard);
    let segments = translated?;
    let saved = state
        .db()?
        .save_meeting_translation(id, target.tag, TRANSLATOR, &segments)
        .await?;
    emit_progress(app, id, MeetingJob::Translate, Some(1.0), None);
    changed(app);
    Ok(saved)
}

/// Sends the segments in bounded batches (each a cancellation and progress point); the model
/// translates every segment on its own and returns one text per segment.
async fn translate_segments(
    app: &AppHandle,
    id: i64,
    detail: &MeetingDetail,
    language: &Language,
) -> AppResult<Vec<TranslatedSegment>> {
    let state = app.state::<AppState>();
    let texts: Vec<&str> = detail
        .segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect();
    let batches = translation_batches(&texts, TRANSLATION_BATCH_CHARS);
    let mut translated = Vec::with_capacity(texts.len());
    for (index, batch) in batches.iter().enumerate() {
        progress(app, id, MeetingJob::Translate, index, batches.len());
        let originals: Vec<String> = texts[batch.clone()]
            .iter()
            .map(|text| (*text).to_owned())
            .collect();
        let on_progress: sv_engine::ProgressCallback = {
            let app = app.clone();
            let steps = batches.len().max(1);
            Box::new(move |fraction: f32, _message: Option<String>| {
                let overall = (index as f32 + fraction.clamp(0.0, 1.0)) / steps as f32;
                emit_progress(
                    &app,
                    id,
                    MeetingJob::Translate,
                    Some(overall.min(0.99)),
                    None,
                );
            })
        };
        let request = state
            .engine
            .translate(TRANSLATOR, language.base, &originals, on_progress);
        let lines = tokio::time::timeout(TRANSLATION_TIMEOUT, request)
            .await
            .map_err(|_| {
                AppError::Other("The translation model did not answer in time.".to_owned())
            })??;
        if lines.len() != originals.len() {
            return Err(AppError::Other(format!(
                "The translation model returned {} lines for {}.",
                lines.len(),
                originals.len()
            )));
        }
        for (segment, line) in detail.segments[batch.clone()].iter().zip(lines) {
            translated.push(TranslatedSegment {
                segment_id: segment.id,
                text: line,
            });
        }
    }
    Ok(translated)
}
