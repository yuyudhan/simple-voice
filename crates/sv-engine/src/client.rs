// FilePath: crates/sv-engine/src/client.rs
//! Request/response correlation over the helper's stdin/stdout.
//!
//! Every request gets a fresh numeric id and a pending entry holding the reply channel and an
//! optional progress callback. Replies are matched by id as the app forwards stdout lines, so
//! any number of requests can be in flight (a model download and a transcription, say). The
//! lock is never held across an await or while a progress callback runs.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, MutexGuard};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use sv_domain::{AppError, AppResult};
use tokio::sync::oneshot;

/// Receives `(fraction 0..1, message)` for each progress event of a request.
pub type ProgressCallback = Box<dyn Fn(f32, Option<String>) + Send + Sync>;

/// Writes one protocol line (without the trailing newline) to the helper's stdin.
pub trait Transport: Send + Sync + 'static {
    fn send_line(&self, line: String) -> AppResult<()>;
}

const STOPPED: &str = "The speech engine stopped unexpectedly";
const NOT_RUNNING: &str = "The speech engine is not running";

struct Pending {
    reply: oneshot::Sender<AppResult<Value>>,
    on_progress: Option<Arc<ProgressCallback>>,
}

#[derive(Default)]
struct Inner {
    transport: Option<Arc<dyn Transport>>,
    next_id: u64,
    pending: HashMap<u64, Pending>,
}

/// Cheap to clone; all clones share one connection and one pending map.
#[derive(Clone, Default)]
pub struct EngineClient {
    inner: Arc<Mutex<Inner>>,
}

impl fmt::Debug for EngineClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let inner = self.lock();
        f.debug_struct("EngineClient")
            .field("connected", &inner.transport.is_some())
            .field("pending", &inner.pending.len())
            .finish()
    }
}

impl EngineClient {
    /// A client with no helper attached; requests fail until [`EngineClient::connect`].
    pub fn new() -> EngineClient {
        EngineClient::default()
    }

    /// Attaches the pipes of a freshly spawned helper.
    pub fn connect(&self, transport: Arc<dyn Transport>) {
        self.lock().transport = Some(transport);
    }

    /// Handles one line the helper wrote to stdout.
    pub fn on_stdout_line(&self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let message: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(%error, line = %preview(line), "ignoring non-JSON engine output");
                return;
            }
        };
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            tracing::warn!(line = %preview(line), "ignoring engine output without an id");
            return;
        };

        if message.get("event").and_then(Value::as_str) == Some("progress") {
            self.dispatch_progress(id, &message);
            return;
        }
        let Some(ok) = message.get("ok").and_then(Value::as_bool) else {
            tracing::warn!(id, line = %preview(line), "ignoring engine output of unknown shape");
            return;
        };
        let Some(pending) = self.lock().pending.remove(&id) else {
            tracing::debug!(id, "ignoring reply for a request that is no longer waiting");
            return;
        };
        let outcome = if ok {
            Ok(message.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let text = message
                .get("error")
                .and_then(Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .unwrap_or("The speech engine reported an unknown error");
            Err(AppError::Engine(text.to_owned()))
        };
        // The requester may have been cancelled in the meantime; nobody is left to tell.
        let _ = pending.reply.send(outcome);
    }

    /// The helper exited: fail everything in flight and refuse new requests until reconnected.
    pub fn on_terminated(&self) {
        let drained: Vec<Pending> = {
            let mut inner = self.lock();
            inner.transport = None;
            inner.pending.drain().map(|(_, pending)| pending).collect()
        };
        for pending in drained {
            let _ = pending
                .reply
                .send(Err(AppError::Engine(STOPPED.to_owned())));
        }
    }

    pub async fn request<T: DeserializeOwned>(
        &self,
        cmd: &str,
        params: Value,
        timeout: Duration,
    ) -> AppResult<T> {
        self.send_request(cmd, params, None, timeout).await
    }

    pub async fn request_with_progress<T: DeserializeOwned>(
        &self,
        cmd: &str,
        params: Value,
        on_progress: ProgressCallback,
        timeout: Duration,
    ) -> AppResult<T> {
        self.send_request(cmd, params, Some(Arc::new(on_progress)), timeout)
            .await
    }

    async fn send_request<T: DeserializeOwned>(
        &self,
        cmd: &str,
        params: Value,
        on_progress: Option<Arc<ProgressCallback>>,
        timeout: Duration,
    ) -> AppResult<T> {
        let mut body = match params {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            other => {
                return Err(AppError::invalid(format!(
                    "Engine command {cmd} needs named parameters, got {other}"
                )))
            }
        };
        let (reply, receiver) = oneshot::channel();
        let (transport, id) = {
            let mut inner = self.lock();
            let transport = inner
                .transport
                .clone()
                .ok_or_else(|| AppError::Engine(NOT_RUNNING.to_owned()))?;
            inner.next_id += 1;
            let id = inner.next_id;
            inner.pending.insert(id, Pending { reply, on_progress });
            (transport, id)
        };
        // Removes the pending entry on every exit path, including timeout and a dropped future.
        let _guard = PendingGuard { client: self, id };

        body.insert("id".to_owned(), Value::from(id));
        body.insert("cmd".to_owned(), Value::from(cmd));
        transport.send_line(Value::Object(body).to_string())?;

        let value = match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(outcome)) => outcome?,
            Ok(Err(_)) => return Err(AppError::Engine(STOPPED.to_owned())),
            Err(_) => {
                return Err(AppError::Engine(format!(
                    "The speech engine did not answer {cmd} within {}",
                    describe(timeout)
                )))
            }
        };
        serde_json::from_value(value).map_err(|error| {
            AppError::Engine(format!(
                "The speech engine sent an unexpected reply to {cmd}: {error}"
            ))
        })
    }

    fn dispatch_progress(&self, id: u64, message: &Value) {
        let callback = match self.lock().pending.get(&id) {
            Some(pending) => pending.on_progress.clone(),
            None => {
                tracing::debug!(
                    id,
                    "ignoring progress for a request that is no longer waiting"
                );
                return;
            }
        };
        let Some(callback) = callback else {
            return;
        };
        let fraction = message
            .get("fraction")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let text = message
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned);
        callback(fraction, text);
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock()
    }
}

struct PendingGuard<'a> {
    client: &'a EngineClient,
    id: u64,
}

impl Drop for PendingGuard<'_> {
    fn drop(&mut self) {
        self.client.lock().pending.remove(&self.id);
    }
}

fn describe(timeout: Duration) -> String {
    let seconds = timeout.as_secs();
    if seconds >= 120 {
        format!("{} min", seconds / 60)
    } else if seconds > 0 {
        format!("{seconds} s")
    } else {
        format!("{} ms", timeout.as_millis())
    }
}

fn preview(line: &str) -> &str {
    match line.char_indices().nth(200) {
        Some((end, _)) => &line[..end],
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use parking_lot::Mutex as SyncMutex;

    use serde::Deserialize;
    use tokio::sync::mpsc;

    use super::*;

    /// Records every line and hands it to a responder task.
    struct ChannelTransport {
        lines: mpsc::UnboundedSender<String>,
    }

    impl Transport for ChannelTransport {
        fn send_line(&self, line: String) -> AppResult<()> {
            self.lines
                .send(line)
                .map_err(|_| AppError::Engine("closed".to_owned()))
        }
    }

    fn connected() -> (EngineClient, mpsc::UnboundedReceiver<String>) {
        let client = EngineClient::new();
        let (tx, rx) = mpsc::unbounded_channel();
        client.connect(Arc::new(ChannelTransport { lines: tx }));
        (client, rx)
    }

    fn id_of(line: &str) -> u64 {
        let value: Value = serde_json::from_str(line).unwrap();
        value["id"].as_u64().unwrap()
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Version {
        version: String,
    }

    #[tokio::test]
    async fn resolves_success_and_sends_flat_request() {
        let (client, mut rx) = connected();
        let feeder = client.clone();
        let responder = tokio::spawn(async move {
            let line = rx.recv().await.unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(request["cmd"], "ping");
            assert_eq!(request["model"], "parakeet-tdt-v3");
            let id = request["id"].as_u64().unwrap();
            feeder.on_stdout_line(&format!(
                "{{\"id\":{id},\"ok\":true,\"result\":{{\"version\":\"1.2.3\"}}}}\n"
            ));
        });
        let params = serde_json::from_str(r#"{"model":"parakeet-tdt-v3"}"#).unwrap();
        let reply: Version = client
            .request("ping", params, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(
            reply,
            Version {
                version: "1.2.3".to_owned()
            }
        );
        responder.await.unwrap();
        assert_eq!(client.lock().pending.len(), 0);
    }

    #[tokio::test]
    async fn surfaces_engine_errors() {
        let (client, mut rx) = connected();
        let feeder = client.clone();
        tokio::spawn(async move {
            let id = id_of(&rx.recv().await.unwrap());
            feeder.on_stdout_line(&format!(
                "{{\"id\":{id},\"ok\":false,\"error\":\"accessibility permission missing\"}}"
            ));
        });
        let result: AppResult<Value> = client
            .request("paste", Value::Null, Duration::from_secs(5))
            .await;
        assert_eq!(
            result,
            Err(AppError::Engine(
                "accessibility permission missing".to_owned()
            ))
        );
    }

    #[tokio::test]
    async fn delivers_progress_in_order_before_the_result() {
        let (client, mut rx) = connected();
        let feeder = client.clone();
        tokio::spawn(async move {
            let id = id_of(&rx.recv().await.unwrap());
            for (fraction, message) in [(0.1, "a"), (0.5, "b"), (1.0, "c")] {
                feeder.on_stdout_line(&format!(
                    "{{\"id\":{id},\"event\":\"progress\",\"fraction\":{fraction},\
                     \"message\":\"{message}\"}}"
                ));
            }
            feeder.on_stdout_line(&format!(
                "{{\"id\":{id},\"event\":\"progress\",\"fraction\":7}}"
            ));
            feeder.on_stdout_line(&format!(
                "{{\"id\":{id},\"ok\":true,\"result\":{{\"status\":\"ready\"}}}}"
            ));
        });
        let seen = Arc::new(SyncMutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let on_progress: ProgressCallback = Box::new(move |fraction, message| {
            sink.lock().push((fraction, message));
        });
        let timeout = Duration::from_secs(5);
        let result: Value = client
            .request_with_progress("download_model", Value::Null, on_progress, timeout)
            .await
            .unwrap();
        assert_eq!(result["status"], "ready");
        let seen = seen.lock().clone();
        assert_eq!(
            seen,
            vec![
                (0.1, Some("a".to_owned())),
                (0.5, Some("b".to_owned())),
                (1.0, Some("c".to_owned())),
                (1.0, None),
            ]
        );
    }

    #[tokio::test]
    async fn ignores_unknown_ids_and_malformed_lines() {
        let (client, mut rx) = connected();
        let feeder = client.clone();
        tokio::spawn(async move {
            let id = id_of(&rx.recv().await.unwrap());
            feeder.on_stdout_line("engine starting up");
            feeder.on_stdout_line("{not json");
            feeder.on_stdout_line("");
            feeder.on_stdout_line(r#"{"ok":true}"#);
            feeder.on_stdout_line(&format!("{{\"id\":{id}}}"));
            feeder.on_stdout_line(r#"{"id":999,"ok":true,"result":{"version":"wrong"}}"#);
            feeder.on_stdout_line(r#"{"id":999,"event":"progress","fraction":0.5}"#);
            feeder.on_stdout_line(&format!(
                "{{\"id\":{id},\"ok\":true,\"result\":{{\"version\":\"right\"}}}}"
            ));
        });
        let reply: Version = client
            .request("ping", Value::Null, Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(reply.version, "right");
    }

    #[tokio::test]
    async fn termination_fails_pending_and_disconnects() {
        let (client, mut rx) = connected();
        let feeder = client.clone();
        tokio::spawn(async move {
            rx.recv().await.unwrap();
            feeder.on_terminated();
        });
        let result: AppResult<Value> = client
            .request("transcribe", Value::Null, Duration::from_secs(5))
            .await;
        assert_eq!(result, Err(AppError::Engine(STOPPED.to_owned())));
        let after: AppResult<Value> = client
            .request("ping", Value::Null, Duration::from_secs(5))
            .await;
        assert_eq!(after, Err(AppError::Engine(NOT_RUNNING.to_owned())));
    }

    #[tokio::test]
    async fn timeout_removes_the_pending_entry() {
        let (client, _rx) = connected();
        let result: AppResult<Value> = client
            .request("preload", Value::Null, Duration::from_millis(30))
            .await;
        assert!(matches!(&result, Err(AppError::Engine(text)) if text.contains("preload")));
        assert_eq!(client.lock().pending.len(), 0);
        // A late reply for the timed-out request is ignored without panicking.
        client.on_stdout_line(r#"{"id":1,"ok":true,"result":{}}"#);
    }

    #[tokio::test]
    async fn disconnected_client_refuses_requests() {
        let client = EngineClient::new();
        let result: AppResult<Value> = client
            .request("ping", Value::Null, Duration::from_secs(1))
            .await;
        assert_eq!(result, Err(AppError::Engine(NOT_RUNNING.to_owned())));
    }

    #[tokio::test]
    async fn rejects_positional_params() {
        let (client, _rx) = connected();
        let result: AppResult<Value> = client
            .request("ping", Value::from(3), Duration::from_secs(1))
            .await;
        assert!(matches!(result, Err(AppError::InvalidInput(_))));
    }

    #[test]
    fn debug_shows_connection_state() {
        let client = EngineClient::new();
        assert_eq!(
            format!("{client:?}"),
            "EngineClient { connected: false, pending: 0 }"
        );
    }
}
