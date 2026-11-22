// FilePath: crates/sv-text/examples/latency/engine.rs
//! The engine helper over its JSON-lines protocol (docs/internal/engine.md), the way the app's
//! `sv-engine` client speaks it: requests carry an id and get one `ok` line back, notifications
//! (`stream_audio`) carry none, and events (`stream_partial`, `stream_failed`) arrive unasked.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde_json::{json, Map, Value};

type Replies = Arc<Mutex<HashMap<u64, Sender<Result<Value, String>>>>>;

/// An unsolicited line about a live stream. The stream only drives the display, so the bench
/// counts its hypotheses and failures without reading their text.
#[derive(Debug)]
pub(crate) enum Event {
    Partial { stream: u64 },
    Failed { stream: u64, reason: String },
}

#[derive(Debug)]
pub(crate) struct Engine {
    child: Mutex<Child>,
    stdin: Mutex<Option<ChildStdin>>,
    replies: Replies,
    next_id: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Engine {
    /// Starts the helper at `path` on `models_dir`; its events go to the returned receiver.
    pub(crate) fn start(path: &str, models_dir: &str) -> Result<(Self, Receiver<Event>), String> {
        let mut child = Command::new(path)
            .args(["--models-dir", models_dir])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("{path}: {error}"))?;
        let stdout = child.stdout.take().ok_or("the helper has no stdout")?;
        let stdin = child.stdin.take().ok_or("the helper has no stdin")?;
        let replies: Replies = Arc::default();
        let (events, receiver) = mpsc::channel();
        let reader_replies = Arc::clone(&replies);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(message) = serde_json::from_str::<Value>(&line) {
                    dispatch(&message, &reader_replies, &events);
                }
            }
        });
        let engine = Self {
            child: Mutex::new(child),
            stdin: Mutex::new(Some(stdin)),
            replies,
            next_id: AtomicU64::new(1),
        };
        Ok((engine, receiver))
    }

    /// Sends a request; its reply arrives on the returned receiver.
    pub(crate) fn request(
        &self,
        cmd: &str,
        params: Value,
    ) -> Result<(u64, Receiver<Result<Value, String>>), String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = mpsc::channel();
        lock(&self.replies).insert(id, sender);
        self.send(Some(id), cmd, params)?;
        Ok((id, receiver))
    }

    /// Sends a request and waits for its result; past `limit` the request is cancelled, as the
    /// app drops a late request.
    pub(crate) fn call(&self, cmd: &str, params: Value, limit: Duration) -> Result<Value, String> {
        let (id, receiver) = self.request(cmd, params)?;
        wait(self, id, &receiver, limit)
    }

    pub(crate) fn notify(&self, cmd: &str, params: Value) -> Result<(), String> {
        self.send(None, cmd, params)
    }

    pub(crate) fn cancel(&self, id: u64) {
        lock(&self.replies).remove(&id);
        let cancel_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        // A cancel for a request that already finished is a no-op in the helper.
        if self
            .send(Some(cancel_id), "cancel", json!({ "target": id }))
            .is_err()
        {
            eprintln!("could not cancel request {id}");
        }
    }

    fn send(&self, id: Option<u64>, cmd: &str, params: Value) -> Result<(), String> {
        let mut body = match params {
            Value::Object(map) => map,
            _ => Map::new(),
        };
        if let Some(id) = id {
            body.insert("id".to_owned(), Value::from(id));
        }
        body.insert("cmd".to_owned(), Value::from(cmd));
        let mut stdin = lock(&self.stdin);
        let stdin = stdin.as_mut().ok_or("the helper was stopped")?;
        writeln!(stdin, "{}", Value::Object(body))
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("the helper stopped reading: {error}"))
    }

    /// Closes the helper's input, which ends it, and waits for it to exit.
    pub(crate) fn stop(&self) {
        lock(&self.stdin).take();
        if let Err(error) = lock(&self.child).wait() {
            eprintln!("the helper did not exit cleanly: {error}");
        }
    }
}

/// Waits up to `limit` for the reply to request `id`, cancelling it when it runs late.
pub(crate) fn wait(
    engine: &Engine,
    id: u64,
    receiver: &Receiver<Result<Value, String>>,
    limit: Duration,
) -> Result<Value, String> {
    match receiver.recv_timeout(limit) {
        Ok(reply) => reply,
        Err(RecvTimeoutError::Timeout) => {
            engine.cancel(id);
            Err(format!("timed out after {:.1}s", limit.as_secs_f64()))
        }
        Err(RecvTimeoutError::Disconnected) => Err("the helper stopped".to_owned()),
    }
}

fn dispatch(message: &Value, replies: &Replies, events: &Sender<Event>) {
    if let (Some(id), Some(ok)) = (message["id"].as_u64(), message["ok"].as_bool()) {
        let reply = if ok {
            Ok(message["result"].clone())
        } else {
            Err(message["error"].as_str().unwrap_or("failed").to_owned())
        };
        if let Some(sender) = lock(replies).remove(&id) {
            // The waiter may have given up on it.
            let _ = sender.send(reply);
        }
        return;
    }
    let stream = message["streamId"].as_u64().unwrap_or_default();
    let text = |key: &str| message[key].as_str().unwrap_or_default().to_owned();
    let event = match message["event"].as_str() {
        Some("stream_partial") => Event::Partial { stream },
        Some("stream_failed") => Event::Failed {
            stream,
            reason: text("reason"),
        },
        _ => return,
    };
    // Nobody listening between clips is fine.
    let _ = events.send(event);
}

/// Standard base64 with padding, for `stream_audio`'s PCM.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk.first().copied().map_or(0, u32::from);
        let b1 = chunk.get(1).copied().map_or(0, u32::from);
        let b2 = chunk.get(2).copied().map_or(0, u32::from);
        let triple = (b0 << 16) | (b1 << 8) | b2;
        let sextet = |shift: u32| char::from(ALPHABET[((triple >> shift) & 0x3F) as usize]);
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(if chunk.len() > 1 { sextet(6) } else { '=' });
        out.push(if chunk.len() > 2 { sextet(0) } else { '=' });
    }
    out
}
