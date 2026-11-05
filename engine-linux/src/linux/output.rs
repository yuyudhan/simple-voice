// FilePath: engine-linux/src/linux/output.rs
//! The protocol stream: one JSON object per line on stdout, each written with a single write so
//! replies from concurrent requests never interleave.

use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::{lock, log};

#[derive(Debug, Default)]
pub(crate) struct Output {
    stdout: Mutex<()>,
}

impl Output {
    pub(crate) fn success(&self, id: u64, result: &Value) {
        self.line(&json!({ "id": id, "ok": true, "result": result }));
    }

    pub(crate) fn failure(&self, id: u64, message: &str) {
        self.line(&json!({ "id": id, "ok": false, "error": message }));
    }

    fn line(&self, value: &Value) {
        let mut text = value.to_string();
        text.push('\n');
        let _serialized = lock(&self.stdout);
        let mut stdout = std::io::stdout().lock();
        if let Err(error) = stdout
            .write_all(text.as_bytes())
            .and_then(|()| stdout.flush())
        {
            log::error(&format!("writing stdout: {error}"));
        }
    }
}

/// Progress events for one request (`download_model`).
#[derive(Debug, Clone)]
pub(crate) struct Progress {
    id: u64,
    output: Arc<Output>,
}

impl Progress {
    pub(crate) fn new(id: u64, output: Arc<Output>) -> Self {
        Self { id, output }
    }

    pub(crate) fn report(&self, fraction: f64, message: &str) {
        self.output.line(&json!({
            "id": self.id,
            "event": "progress",
            "fraction": fraction.clamp(0.0, 1.0),
            "message": message,
        }));
    }
}
