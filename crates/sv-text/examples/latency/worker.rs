// FilePath: crates/sv-text/examples/latency/worker.rs
//! The speculative stop of one clip: the clip is replayed in real time (to the live stream,
//! for the display) while a worker drafts the recording so far with the final pass's model and
//! formats closed segments with the app's `Speculator`; at the end of the audio the stop
//! cancels a draft in flight, transcribes, formats, waits for a segment in flight (as
//! `speculative.rs` does), reuses what matches and formats the tail. [`crate::run`] holds the
//! setup both stops share and today's stop.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use sv_text::speculation::{self, Request, Speculator};
use sv_text::PolishOutcome;

use crate::engine::{self, Event};
use crate::run::{lock, millis, replay, reply, will_post_process, Setup, Speculated, Stop, SLOW};
use crate::wav::SAMPLE_RATE;

/// As `speculative.rs`: the stop waits this long for a segment in flight.
const FINISH_WAIT: Duration = Duration::from_secs(2);
/// How often an idle worker side looks again.
const POLL: Duration = Duration::from_millis(50);

/// What the worker saw of one clip.
#[derive(Debug, Default)]
struct Seen {
    partials: usize,
    failed: usize,
    drafts: usize,
    draft_ms: u64,
    /// The last draft, to compare with the whole recording's transcript.
    last_draft: String,
}

impl Seen {
    fn take(&mut self, event: Event, stream: u64) {
        match event {
            Event::Partial { stream: id, .. } if id == stream => self.partials += 1,
            Event::Failed { stream: id, reason } if id == stream => {
                self.failed += 1;
                eprintln!("live stream failed: {reason}");
            }
            Event::Partial { .. } | Event::Failed { .. } => {}
        }
    }
}

/// What the worker shares with the clip's replay and stop.
#[derive(Debug)]
struct Shared {
    speculator: Mutex<Speculator>,
    /// The audio replayed so far: the recording as long as the user has spoken.
    recorded: Mutex<Vec<i16>>,
    stopped: AtomicBool,
    /// The draft and the segment request the worker is waiting on.
    draft: Mutex<Option<u64>>,
    segment: Mutex<Option<u64>>,
}

/// What the drafting side saw of one clip.
#[derive(Debug, Default)]
struct Drafted {
    drafts: usize,
    draft_ms: u64,
    last: String,
}

/// The app's worker, on a thread: drafts of the recording so far when due, side by side with
/// formatting the next closed segment, one request at a time, prepared again after each.
fn work(setup: &Setup, events: &Receiver<Event>, stream: u64, shared: &Shared) -> Seen {
    std::thread::scope(|scope| {
        let drafting = scope.spawn(|| draft(setup, shared));
        let mut seen = format(setup, events, stream, shared);
        if let Ok(drafted) = drafting.join() {
            seen.drafts = drafted.drafts;
            seen.draft_ms = drafted.draft_ms;
            seen.last_draft = drafted.last;
        }
        seen
    })
}

/// The drafting loop: the recording so far, transcribed whenever `draft_due`.
fn draft(setup: &Setup, shared: &Shared) -> Drafted {
    let mut drafted = Drafted::default();
    let draft_wav = std::env::temp_dir().join(format!("latency-draft-{}.wav", std::process::id()));
    let (mut drafted_ms, mut cost_ms) = (0, 0);
    while !shared.stopped.load(Ordering::SeqCst) {
        let recorded = lock(&shared.recorded).clone();
        let audio_ms =
            u64::try_from(recorded.len()).unwrap_or(u64::MAX) * 1000 / u64::from(SAMPLE_RATE);
        if !speculation::draft_due(audio_ms, drafted_ms, cost_ms) {
            std::thread::sleep(POLL);
            continue;
        }
        let started = Instant::now();
        let draft = crate::wav::write(&draft_wav, &recorded)
            .and_then(|()| setup.transcribe(&draft_wav, Some(&shared.draft)));
        cost_ms = millis(started);
        drafted_ms = audio_ms;
        match draft {
            Ok(text) => {
                drafted.drafts += 1;
                drafted.draft_ms += cost_ms;
                lock(&shared.speculator).observe(&text);
                drafted.last = text;
            }
            Err(error) => eprintln!("draft: {error}"),
        }
    }
    let _ = std::fs::remove_file(&draft_wav);
    drafted
}

/// The formatting loop: the next closed segment, one request at a time.
fn format(setup: &Setup, events: &Receiver<Event>, stream: u64, shared: &Shared) -> Seen {
    let mut seen = Seen::default();
    loop {
        for event in events.try_iter() {
            seen.take(event, stream);
        }
        if shared.stopped.load(Ordering::SeqCst) {
            return seen;
        }
        let request = lock(&shared.speculator).next_request(|text| setup.format(text));
        let Some(Request { id, context, input }) = request else {
            match events.recv_timeout(POLL) {
                Ok(event) => seen.take(event, stream),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return seen,
            }
            continue;
        };
        let prompt = setup.prompt(&context, &input);
        let answer = setup
            .generate_request(&prompt)
            .and_then(|(request, receiver)| {
                *lock(&shared.segment) = Some(request);
                let limit = sv_text::polish_timeout(&input);
                let answer = engine::wait(&setup.engine, request, &receiver, limit);
                *lock(&shared.segment) = None;
                answer
            });
        let output = answer.and_then(|answer| {
            let (text, finished) = reply(answer);
            match speculation::accept_segment(&context, &input, &text, finished) {
                PolishOutcome::Polished(text) => Ok(text),
                PolishOutcome::Skipped(reason) => Err(format!("{reason}: {input:?} -> {text:?}")),
            }
        });
        if let Err(reason) = &output {
            eprintln!("segment not formatted while speaking: {reason}");
        }
        lock(&shared.speculator).complete(id, output.ok());
        setup.prepare();
    }
}

/// The speculative stop of one clip, its audio replayed in real time to the live stream (the
/// display, as in the app) and to the worker. Gives `events` back.
pub(crate) fn speculative(
    setup: &Arc<Setup>,
    events: Receiver<Event>,
    wav: &Path,
    samples: &[i16],
) -> (Result<Speculated, String>, Receiver<Event>) {
    for _stale in events.try_iter() {}
    // The worker prepares the model at the recording start.
    setup.prepare();
    let stream = match setup.stream_start() {
        Ok(stream) => stream,
        Err(error) => return (Err(error), events),
    };
    let shared = Arc::new(Shared {
        speculator: Mutex::new(Speculator::new()),
        recorded: Mutex::new(Vec::with_capacity(samples.len())),
        stopped: AtomicBool::new(false),
        draft: Mutex::new(None),
        segment: Mutex::new(None),
    });
    let (done, finished) = mpsc::channel();
    let worker = {
        let (setup, shared) = (Arc::clone(setup), Arc::clone(&shared));
        std::thread::spawn(move || {
            let seen = work(&setup, &events, stream, &shared);
            let _ = done.send(());
            (seen, events)
        })
    };
    let streamed = replay(setup, stream, samples, Some(&shared.recorded));
    let stop_at = Instant::now();
    shared.stopped.store(true, Ordering::SeqCst);
    let outcome = streamed.and_then(|()| finish(setup, wav, stream, stop_at, &shared, &finished));
    let (seen, events) = match worker.join() {
        Ok(joined) => joined,
        Err(_) => {
            return (
                Err("the worker thread panicked".to_owned()),
                mpsc::channel().1,
            )
        }
    };
    let outcome = outcome.map(|mut speculated| {
        speculated.partials = seen.partials;
        speculated.stream_failed = seen.failed;
        speculated.drafts = seen.drafts;
        speculated.draft_ms = seen.draft_ms;
        speculated.draft_text = seen.last_draft;
        speculated
    });
    (outcome, events)
}

/// The stop after streaming, as `pipeline.rs` with a worker runs it: a draft in flight is
/// cancelled at once, a segment in flight is waited for at most [`FINISH_WAIT`].
fn finish(
    setup: &Setup,
    wav: &Path,
    stream: u64,
    stop_at: Instant,
    shared: &Shared,
    finished: &Receiver<()>,
) -> Result<Speculated, String> {
    if let Some(request) = *lock(&shared.draft) {
        setup.engine.cancel(request);
    }
    setup
        .engine
        .call("stream_cancel", json!({ "streamId": stream }), SLOW)?;
    let heard = setup.transcribe(wav, None)?;
    let asr_ms = millis(stop_at);
    let whole = setup.format(&heard);
    if finished.recv_timeout(FINISH_WAIT).is_err() {
        if let Some(request) = lock(&shared.segment).take() {
            setup.engine.cancel(request);
        }
    }
    let (formatted, revised) = {
        let speculator = lock(&shared.speculator);
        (speculator.formatted(), speculator.revised())
    };
    let plan = speculation::plan(&whole, &formatted);
    let (text, note) = if !will_post_process(&whole) {
        (whole.clone(), "not run".to_owned())
    } else if plan.reused.is_empty() {
        let (text, note) = setup.whole_pass(&whole);
        (text, format!("no reuse {note}"))
    } else if plan.spans_a_list() {
        let (text, note) = setup.whole_pass(&whole);
        (text, format!("list spans segments {note}"))
    } else {
        tail_pass(setup, &whole, &plan)
    };
    Ok(Speculated {
        stop: Stop {
            ms: millis(stop_at),
            asr_ms,
            text,
            note: note.trim().to_owned(),
        },
        segments: formatted.len(),
        reused: plan.reused.len(),
        reused_words: plan.reused_words,
        tail_words: plan.tail.split_whitespace().count(),
        revised,
        heard,
        whole,
        ..Speculated::default()
    })
}

/// Reused segments plus the formatted tail, or the whole-text pass when that fails.
fn tail_pass(setup: &Setup, whole: &str, plan: &speculation::Plan) -> (String, String) {
    let tail = if plan.tail.is_empty() || !will_post_process(&plan.tail) {
        Ok(plan.tail.clone())
    } else {
        setup.format_part(&plan.context, &plan.tail)
    };
    let joined = tail.and_then(|tail| {
        let mut parts: Vec<&str> = plan.reused.iter().map(String::as_str).collect();
        parts.push(&tail);
        match sv_text::accept_polish(whole, &speculation::join(&parts), true) {
            PolishOutcome::Polished(text) => Ok(text),
            PolishOutcome::Skipped(reason) => Err(reason),
        }
    });
    match joined {
        Ok(text) => (text, String::new()),
        Err(reason) => {
            let (text, note) = setup.whole_pass(whole);
            (text, format!("tail refused ({reason}) {note}"))
        }
    }
}
