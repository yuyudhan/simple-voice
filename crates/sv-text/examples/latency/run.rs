// FilePath: crates/sv-text/examples/latency/run.rs
//! One clip, stopped two ways. Today's stop: transcribe the whole recording, format, run the
//! whole-text pass (the model prepared at the stop, as `pipeline.rs` does without the worker).
//! Speculative: [`crate::worker`], on the setup and replay here. Both time from the end of the
//! audio to the text that would be pasted.

use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sv_domain::Style;
use sv_text::speculation;
use sv_text::{PolishOutcome, PolishPrompt, PolishTarget, Vocabulary};

use crate::engine::{self, Engine};
use crate::wav::SAMPLE_RATE;

/// The app's `stream_audio` batches: 200 ms.
const BATCH_SAMPLES: usize = 3_200;
const MAX_TOKENS: u32 = 2048;
pub(crate) const SLOW: Duration = Duration::from_secs(120);
const APPLE_INTELLIGENCE: &str = "apple-intelligence";

/// The models and the text settings every clip runs with.
#[derive(Debug)]
pub(crate) struct Setup {
    pub(crate) engine: Engine,
    pub(crate) asr: String,
    pub(crate) formatter: String,
    pub(crate) vocabulary: Vocabulary,
    pub(crate) style: Style,
    pub(crate) languages: Vec<String>,
}

/// What one way of stopping pasted, and how long it took after the audio ended.
#[derive(Debug, Default)]
pub(crate) struct Stop {
    pub(crate) ms: u64,
    pub(crate) asr_ms: u64,
    pub(crate) text: String,
    /// Why the stop did not take its normal path; empty when it did.
    pub(crate) note: String,
}

#[derive(Debug, Default)]
pub(crate) struct Speculated {
    pub(crate) stop: Stop,
    pub(crate) segments: usize,
    pub(crate) reused: usize,
    pub(crate) reused_words: usize,
    pub(crate) tail_words: usize,
    pub(crate) revised: usize,
    pub(crate) partials: usize,
    pub(crate) stream_failed: usize,
    /// Drafts of the recording so far, and their total time.
    pub(crate) drafts: usize,
    pub(crate) draft_ms: u64,
    /// The last draft, to compare with the whole recording's transcript.
    pub(crate) draft_text: String,
    /// The whole recording's transcript, before formatting.
    pub(crate) heard: String,
    /// The deterministic formatter's output for `heard`: what both stops start from.
    pub(crate) whole: String,
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
}

impl Setup {
    fn target(&self) -> PolishTarget {
        if self.formatter == APPLE_INTELLIGENCE {
            PolishTarget::OnDevice
        } else {
            PolishTarget::Chat
        }
    }

    pub(crate) fn format(&self, text: &str) -> String {
        sv_text::format(text, &self.vocabulary, self.style).text
    }

    /// The segment prompt; with no context it is exactly the whole-text pass's prompt.
    pub(crate) fn prompt(&self, context: &str, text: &str) -> PolishPrompt {
        speculation::polish_segment_prompt(
            context,
            text,
            &self.vocabulary.terms,
            self.style,
            self.target(),
            &self.languages,
        )
    }

    fn prepare_request(&self) -> Result<(u64, Receiver<Result<Value, String>>), String> {
        let prompt = self.prompt("", "");
        self.engine.request(
            "prepare_generate",
            json!({ "model": self.formatter, "system": prompt.system, "shots": shots(&prompt) }),
        )
    }

    pub(crate) fn prepare(&self) {
        let prepared = self
            .prepare_request()
            .and_then(|(id, receiver)| engine::wait(&self.engine, id, &receiver, SLOW));
        if let Err(error) = prepared {
            eprintln!("prepare_generate: {error}");
        }
    }

    pub(crate) fn generate_request(
        &self,
        prompt: &PolishPrompt,
    ) -> Result<(u64, Receiver<Result<Value, String>>), String> {
        self.engine.request(
            "generate",
            json!({
                "model": self.formatter,
                "system": prompt.system,
                "shots": shots(prompt),
                "user": prompt.user,
                "maxTokens": MAX_TOKENS,
                "temperature": 0.0,
            }),
        )
    }

    /// Formats `input` after `context` within the app's budget, through the segment guards.
    pub(crate) fn format_part(&self, context: &str, input: &str) -> Result<String, String> {
        let prompt = self.prompt(context, input);
        let (id, receiver) = self.generate_request(&prompt)?;
        let limit = sv_text::polish_timeout(input);
        let (text, finished) = reply(engine::wait(&self.engine, id, &receiver, limit)?);
        match speculation::accept_segment(context, input, &text, finished) {
            PolishOutcome::Polished(text) => Ok(text),
            PolishOutcome::Skipped(reason) => Err(reason),
        }
    }

    /// The whole-text pass: the formatted text, or the deterministic text and why.
    pub(crate) fn whole_pass(&self, whole: &str) -> (String, String) {
        let prompt = self.prompt("", whole);
        let answer = self.generate_request(&prompt).and_then(|(id, receiver)| {
            engine::wait(&self.engine, id, &receiver, sv_text::polish_timeout(whole))
        });
        let outcome = match answer {
            Ok(answer) => {
                let (text, finished) = reply(answer);
                sv_text::accept_polish(whole, &text, finished)
            }
            Err(reason) => PolishOutcome::Skipped(reason),
        };
        match outcome {
            PolishOutcome::Polished(text) => (text, String::new()),
            PolishOutcome::Skipped(reason) => (whole.to_owned(), format!("unformatted: {reason}")),
        }
    }

    /// Transcribes `wav` as the final pass does; `track` records the request as a draft in
    /// flight, for the stop to cancel.
    pub(crate) fn transcribe(
        &self,
        wav: &Path,
        track: Option<&Mutex<Option<u64>>>,
    ) -> Result<String, String> {
        let params = json!({
            "model": self.asr,
            "wavPath": wav.to_string_lossy(),
            "languages": self.languages,
            "vocabulary": self.vocabulary.boost_terms(),
        });
        let (id, receiver) = self.engine.request("transcribe", params)?;
        if let Some(track) = track {
            *lock(track) = Some(id);
        }
        let result = engine::wait(&self.engine, id, &receiver, SLOW);
        if let Some(track) = track {
            *lock(track) = None;
        }
        Ok(result?["text"].as_str().unwrap_or_default().to_owned())
    }

    /// Today's stop with live transcript off: prepare at the stop, transcribe, format, whole-text
    /// pass.
    pub(crate) fn today(&self, wav: &Path) -> Result<Stop, String> {
        self.stop_today(wav, Instant::now())
    }

    /// Today's stop with live transcript on (the default): the clip streams in real time for
    /// the display only, and the stop cancels the stream before it transcribes.
    pub(crate) fn today_streamed(&self, wav: &Path, samples: &[i16]) -> Result<Stop, String> {
        let stream = self.stream_start()?;
        replay(self, stream, samples, None)?;
        let stopped = Instant::now();
        self.engine
            .call("stream_cancel", json!({ "streamId": stream }), SLOW)?;
        self.stop_today(wav, stopped)
    }

    pub(crate) fn stream_start(&self) -> Result<u64, String> {
        let params = json!({ "model": self.asr, "overlay": false });
        let result = self.engine.call("stream_start", params, SLOW)?;
        result["streamId"]
            .as_u64()
            .ok_or_else(|| "stream_start: no streamId".to_owned())
    }

    fn stop_today(&self, wav: &Path, stopped: Instant) -> Result<Stop, String> {
        // `pipeline.rs` spawns the prepare and transcribes meanwhile.
        let _prepare = self.prepare_request()?;
        let heard = self.transcribe(wav, None)?;
        let asr_ms = millis(stopped);
        let whole = self.format(&heard);
        let (text, note) = if will_post_process(&whole) {
            self.whole_pass(&whole)
        } else {
            (whole, "not run".to_owned())
        };
        Ok(Stop {
            ms: millis(stopped),
            asr_ms,
            text,
            note,
        })
    }

    /// Loads the models' lazy parts (the vocabulary booster, the prompt) before the first clip
    /// is timed.
    pub(crate) fn warm_up(&self, wav: &Path) -> Result<(), String> {
        let whole = self.format(&self.transcribe(wav, None)?);
        self.prepare();
        self.whole_pass(&whole);
        Ok(())
    }
}

fn shots(prompt: &PolishPrompt) -> Value {
    prompt
        .shots
        .iter()
        .map(|(user, assistant)| json!({ "user": user, "assistant": assistant }))
        .collect()
}

pub(crate) fn reply(answer: Value) -> (String, bool) {
    let text = answer["text"].as_str().unwrap_or_default().to_owned();
    (text, answer["finished"].as_bool().unwrap_or(true))
}

pub(crate) fn will_post_process(text: &str) -> bool {
    !text.trim().is_empty() && sv_text::should_skip_polish(text).is_none()
}

/// Sends the clip to the live stream in 200 ms batches at the pace it was spoken, adding each
/// to `recorded`, then waits out the last batch: the user stops when the speech ends.
pub(crate) fn replay(
    setup: &Setup,
    stream: u64,
    samples: &[i16],
    recorded: Option<&Mutex<Vec<i16>>>,
) -> Result<(), String> {
    let start = Instant::now();
    for (index, batch) in samples.chunks(BATCH_SAMPLES).enumerate() {
        let offset = u64::try_from(index * BATCH_SAMPLES).unwrap_or(u64::MAX);
        sleep_until(start + Duration::from_millis(offset * 1000 / u64::from(SAMPLE_RATE)));
        if let Some(recorded) = recorded {
            lock(recorded).extend_from_slice(batch);
        }
        let bytes: Vec<u8> = batch
            .iter()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        let pcm = engine::base64(&bytes);
        setup
            .engine
            .notify("stream_audio", json!({ "streamId": stream, "pcm": pcm }))?;
    }
    let total = u64::try_from(samples.len()).unwrap_or(u64::MAX);
    sleep_until(start + Duration::from_millis(total * 1000 / u64::from(SAMPLE_RATE)));
    Ok(())
}

fn sleep_until(due: Instant) {
    if let Some(wait) = due.checked_duration_since(Instant::now()) {
        std::thread::sleep(wait);
    }
}
