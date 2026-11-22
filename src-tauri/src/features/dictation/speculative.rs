// FilePath: src-tauri/src/features/dictation/speculative.rs
//! Formatting while the user still speaks (requirements LT-5). With the live transcript on, an
//! on-device voice model and an on-device formatting model, a worker starts with the
//! recording: it builds the dictation's vocabulary (dictionary plus the focused field's hints)
//! and has the model read its instructions. Then, every few seconds, it transcribes the
//! recording so far again with the model and options the stop will use (a "draft"), and side by
//! side formats each closed segment of settled draft words (`sv_text::speculation`), one request
//! at a time, with the segments before it as context. The live stream's own hypotheses are not used:
//! its sliding windows hear differently from the whole pass, so little of it would match. At
//! stop the pasted words still come from the whole recording's transcription: the segments
//! whose words match it are reused and only the rest goes to the model. Whatever the reuse
//! cannot cover safely (snippets, a list spanning segments, a refused tail) takes the whole-text
//! pass instead, as without the worker.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sv_domain::models::{groq_chat_model, SMART_SELECT};
use sv_domain::{AppResult, Settings, Snippet, TranscriptionProvider};
use sv_text::speculation::{self, Request, Speculator};
use sv_text::{PolishOutcome, Vocabulary};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::watch;

use super::context::{self, ContextTask};
use super::draft_file::DraftFile;
use super::transcription::{engine_transcribe, ready_route, vocabulary};
use crate::features::llm;
use crate::state::{lock, AppState};

/// As the whole-text pass: a tail can be most of a long dictation.
const MAX_TOKENS: u32 = 2048;
/// Longest the stop waits for a segment that was being formatted when the recording stopped:
/// about what formatting its words again would take.
const FINISH_WAIT: Duration = Duration::from_secs(2);
/// How often an idle worker looks at the recording again, and a worker paused for the live
/// stream's backlog at the backlog.
const POLL: Duration = Duration::from_millis(100);
/// The recorder's sample rate.
const SAMPLES_PER_MS: usize = 16;

/// What the recording hands the worker: its audio so far, and whether the live stream's audio
/// queue is backing up (the worker then waits, so the stream keeps up). Dropping `recording`
/// (the stream ended) tells the worker the recording stopped.
#[derive(Debug)]
pub(crate) struct Feed {
    pub(crate) recording: watch::Sender<()>,
    pub(crate) audio: Arc<Mutex<Vec<i16>>>,
    pub(crate) backlog: Arc<AtomicBool>,
}

/// One recording's worker.
#[derive(Debug)]
pub(crate) struct Speculation {
    feed: Option<Feed>,
    vocabulary: watch::Receiver<Option<Arc<Vocabulary>>>,
    speculator: Arc<Mutex<Speculator>>,
    worker: JoinHandle<()>,
    draft: DraftFile,
}

/// Whether a recording with these settings formats while it records: it needs the live stream
/// and on-device models for both passes (neither audio nor partial text goes to Groq while the
/// user speaks).
fn eligible(settings: &Settings) -> bool {
    settings.live_transcript
        && settings.transcription_provider != TranscriptionProvider::Groq
        && llm::post_processing_model(settings)
            .is_some_and(|model| groq_chat_model(&model).is_none())
}

/// Starts the worker of recording `id` when it formats while it records, taking its context
/// read; `None`, with the context read left in place, otherwise.
pub(crate) fn start(
    app: &AppHandle,
    id: u64,
    settings: &Settings,
    context: &mut Option<ContextTask>,
) -> Option<Speculation> {
    if !eligible(settings) {
        return None;
    }
    let draft = match sv_storage::paths::audio_dir() {
        Ok(dir) => DraftFile::new(&dir, id),
        Err(error) => {
            tracing::info!(%error, "no place for drafts; formatting after the stop");
            return None;
        }
    };
    let (recording, stopped) = watch::channel(());
    let (vocabulary_sender, vocabulary) = watch::channel(None);
    let audio = Arc::new(Mutex::new(Vec::new()));
    let backlog = Arc::new(AtomicBool::new(false));
    let speculator = Arc::new(Mutex::new(Speculator::new()));
    let worker = tauri::async_runtime::spawn(work(
        app.clone(),
        settings.clone(),
        context.take(),
        vocabulary_sender,
        Drafts {
            file: draft.clone(),
            audio: Arc::clone(&audio),
            stopped,
        },
        Arc::clone(&speculator),
        Arc::clone(&backlog),
    ));
    Some(Speculation {
        feed: Some(Feed {
            recording,
            audio,
            backlog,
        }),
        vocabulary,
        speculator,
        worker,
        draft,
    })
}

impl Speculation {
    /// The feed for the recording and its live stream; taken once.
    pub(crate) fn take_feed(&mut self) -> Option<Feed> {
        self.feed.take()
    }

    /// The vocabulary the worker built at the start; `None` when it could not.
    pub(crate) async fn vocabulary(&mut self) -> Option<Vocabulary> {
        let built = self.vocabulary.wait_for(Option::is_some).await;
        built.ok().and_then(|built| built.as_deref().cloned())
    }

    /// The formatted text for `whole` (the deterministic formatter's output for the whole
    /// transcript) and the model that formatted it, from the segments the worker formatted plus
    /// the tail after them; `None` when the whole-text pass has to run instead. Called once the
    /// live stream ended, so the worker starts nothing new.
    pub(crate) async fn finish(
        mut self,
        state: &AppState,
        settings: &Settings,
        vocabulary: &Vocabulary,
        snippets: &[Snippet],
        whole: &str,
    ) -> Option<(String, String)> {
        if tokio::time::timeout(FINISH_WAIT, &mut self.worker)
            .await
            .is_err()
        {
            // Stopping the worker drops its request, which cancels it in the helper, so the
            // tail does not queue behind a sentence that is no longer waited for.
            tracing::info!("formatting a segment ran past the stop; cancelled");
            self.worker.abort();
            let _ = (&mut self.worker).await;
        }
        let (formatted, revised) = {
            let speculator = lock(&self.speculator);
            (speculator.formatted(), speculator.revised())
        };
        let model = llm::post_processing_model(settings)?;
        // Payloads are protected over the whole text; segments carry none.
        if sv_text::snippets::protect(whole, snippets).hits() > 0 {
            tracing::info!("snippets in the dictation; formatting the whole text");
            return None;
        }
        let plan = speculation::plan(whole, &formatted);
        tracing::info!(
            segments = formatted.len(),
            reused = plan.reused.len(),
            reused_words = plan.reused_words,
            tail_words = plan.tail.split_whitespace().count(),
            revised,
            "formatted while speaking"
        );
        if plan.reused.is_empty() {
            return None;
        }
        if plan.spans_a_list() {
            tracing::info!("a list may span segments; formatting the whole text");
            return None;
        }
        let tail = if plan.tail.is_empty() || sv_text::should_skip_polish(&plan.tail).is_some() {
            plan.tail.clone()
        } else {
            let request = Request {
                id: 0,
                context: plan.context.clone(),
                input: plan.tail.clone(),
            };
            match format_segment(state, settings, &model, vocabulary, &request).await {
                Ok(text) => text,
                Err(reason) => {
                    tracing::info!(
                        reason,
                        "the tail was not formatted; formatting the whole text"
                    );
                    return None;
                }
            }
        };
        let mut parts: Vec<&str> = plan.reused.iter().map(String::as_str).collect();
        parts.push(&tail);
        match sv_text::accept_polish(whole, &speculation::join(&parts), true) {
            PolishOutcome::Polished(text) => Some((text, model)),
            PolishOutcome::Skipped(reason) => {
                tracing::info!(reason, "joined segments refused; formatting the whole text");
                None
            }
        }
    }
}

impl Drop for Speculation {
    /// A recording abandoned or finished leaves no request running for it, and no draft audio
    /// (removed after any write still in flight).
    fn drop(&mut self) {
        self.worker.abort();
        self.draft.remove();
    }
}

/// The recording's audio as the worker drafts it.
struct Drafts {
    file: DraftFile,
    audio: Arc<Mutex<Vec<i16>>>,
    /// Closed when the recording stopped.
    stopped: watch::Receiver<()>,
}

async fn work(
    app: AppHandle,
    settings: Settings,
    context: Option<ContextTask>,
    vocabulary_sender: watch::Sender<Option<Arc<Vocabulary>>>,
    drafts: Drafts,
    speculator: Arc<Mutex<Speculator>>,
    backlog: Arc<AtomicBool>,
) {
    let Drafts {
        file,
        audio,
        stopped,
    } = drafts;
    let state = app.state::<AppState>();
    let mut built = vocabulary(&state, &settings).await;
    built.add_context_terms(&context::collect(context).await);
    let built = Arc::new(built);
    vocabulary_sender.send_replace(Some(Arc::clone(&built)));
    let (Some(model), Some(voice)) = (
        llm::post_processing_model(&settings),
        draft_model(&state, &settings).await,
    ) else {
        return;
    };
    // The formatting model reads its instructions while the user speaks, not after the stop.
    llm::prepare_model(&state, &model, &prompt(&settings, &model, &built, "", "")).await;
    let worker = Worker {
        state: &state,
        settings: &settings,
        vocabulary: &built,
        speculator: &speculator,
        backlog: &backlog,
    };
    // Drafting and formatting run side by side: the helper answers them on different models.
    tokio::join!(
        worker.draft_loop(&voice, &file, &audio, stopped.clone()),
        worker.format_loop(&model, stopped),
    );
}

/// What both sides of a recording's worker share.
struct Worker<'a> {
    state: &'a AppState,
    settings: &'a Settings,
    vocabulary: &'a Vocabulary,
    speculator: &'a Mutex<Speculator>,
    backlog: &'a AtomicBool,
}

impl Worker<'_> {
    /// Whether the side should wait: the live stream's audio queue is backing up, so the
    /// helper gets to catch up first.
    async fn yield_to_stream(&self) -> bool {
        if self.backlog.load(Ordering::Relaxed) {
            tokio::time::sleep(POLL).await;
            return true;
        }
        false
    }

    /// Drafts the recording so far on `voice` whenever one is due, until the recording stops
    /// (which cancels a draft in flight).
    async fn draft_loop(
        &self,
        voice: &str,
        file: &DraftFile,
        audio: &Mutex<Vec<i16>>,
        mut stopped: watch::Receiver<()>,
    ) {
        let mut recorded: Vec<i16> = Vec::new();
        let (mut drafted_ms, mut cost_ms) = (0, 0);
        while stopped.has_changed().is_ok() {
            if self.yield_to_stream().await {
                continue;
            }
            if let Some(new) = lock(audio).get(recorded.len()..) {
                recorded.extend_from_slice(new);
            }
            let audio_ms = u64::try_from(recorded.len() / SAMPLES_PER_MS).unwrap_or(u64::MAX);
            if !speculation::draft_due(audio_ms, drafted_ms, cost_ms) {
                tokio::time::sleep(POLL).await;
                continue;
            }
            let started = Instant::now();
            let draft = draft(
                self.state,
                self.settings,
                self.vocabulary,
                voice,
                file,
                &recorded,
            );
            let drafted = tokio::select! {
                drafted = draft => drafted,
                () = until_closed(&mut stopped) => return,
            };
            cost_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
            drafted_ms = audio_ms;
            match drafted {
                Ok(text) => lock(self.speculator).observe(&text),
                Err(error) => tracing::info!(%error, voice, "draft not transcribed"),
            }
        }
    }

    /// Formats closed segments on `model`, one at a time, until the recording stops; a segment
    /// in flight then is left for the stop to wait for.
    async fn format_loop(&self, model: &str, stopped: watch::Receiver<()>) {
        let style = self.settings.style;
        while stopped.has_changed().is_ok() {
            if self.yield_to_stream().await {
                continue;
            }
            let request = lock(self.speculator)
                .next_request(|text| sv_text::format(text, self.vocabulary, style).text);
            let Some(request) = request else {
                tokio::time::sleep(POLL).await;
                continue;
            };
            let reply =
                format_segment(self.state, self.settings, model, self.vocabulary, &request).await;
            if let Err(reason) = &reply {
                tracing::info!(reason, "segment not formatted while speaking");
            }
            lock(self.speculator).complete(request.id, reply.ok());
            // A prepared session answers one request; the next one gets a fresh one.
            let next = prompt(self.settings, model, self.vocabulary, "", "");
            llm::prepare_model(self.state, model, &next).await;
        }
    }
}

/// Returns once the sender of `receiver` is gone.
async fn until_closed(receiver: &mut watch::Receiver<()>) {
    while receiver.changed().await.is_ok() {}
}

/// The on-device model drafts run on: the one the whole recording's transcription tries first
/// (Smart Select's first ready model, or the chosen model), so a draft hears what the stop
/// will. `None` with Groq chosen: audio never leaves the Mac while the user speaks.
async fn draft_model(state: &AppState, settings: &Settings) -> Option<String> {
    if settings.transcription_provider == TranscriptionProvider::Groq {
        return None;
    }
    let model = settings.transcription_model.as_str();
    if model == SMART_SELECT {
        return ready_route(state, settings)
            .await
            .ok()
            .map(|route| route.first.to_owned());
    }
    Some(model.to_owned())
}

/// A draft: `samples` (the recording so far) written to `file` and transcribed on `model` as the
/// whole recording will be. A draft only steers formatting while speaking, so its failures stay
/// out of the model's health.
async fn draft(
    state: &AppState,
    settings: &Settings,
    vocabulary: &Vocabulary,
    model: &str,
    file: &DraftFile,
    samples: &[i16],
) -> AppResult<String> {
    file.write(samples).await?;
    let transcript = engine_transcribe(state, settings, vocabulary, file.path(), model).await?;
    Ok(transcript.text)
}

fn prompt(
    settings: &Settings,
    model: &str,
    vocabulary: &Vocabulary,
    context: &str,
    text: &str,
) -> sv_text::PolishPrompt {
    speculation::polish_segment_prompt(
        context,
        text,
        &vocabulary.terms,
        settings.style,
        llm::target(model),
        &settings.languages,
    )
}

/// Formats one segment (or the tail) on `model`; the reply after the guards, or why not.
async fn format_segment(
    state: &AppState,
    settings: &Settings,
    model: &str,
    vocabulary: &Vocabulary,
    request: &Request,
) -> Result<String, String> {
    let prompt = prompt(
        settings,
        model,
        vocabulary,
        &request.context,
        &request.input,
    );
    let limit = sv_text::polish_timeout(&request.input);
    let reply = llm::generate(state, model, &prompt, MAX_TOKENS, limit).await?;
    match speculation::accept_segment(
        &request.context,
        &request.input,
        &reply.text,
        reply.finished,
    ) {
        PolishOutcome::Polished(text) => Ok(text),
        PolishOutcome::Skipped(reason) => Err(reason),
    }
}
