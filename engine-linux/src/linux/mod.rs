// FilePath: engine-linux/src/linux/mod.rs
//! The Linux helper. Every request runs in its own task, so a long transcription or download
//! never delays `permissions` or `paste`, and `cancel` can stop it; replies are matched by `id`.
//! A line without an `id` is a notification and gets no reply. stdout carries protocol lines
//! only (whisper.cpp logs to stderr); the helper exits when stdin closes.

mod autostart;
mod desktop;
mod log;
mod models;
mod mute;
mod output;
mod router;
mod tools;
mod whisper;

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::AbortHandle;

use self::output::Progress;
use self::router::Engine;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");
/// How long requests still running when stdin closes get to answer before they are aborted, and
/// what an aborted transcription then gets to notice its cancel flag before the process exits.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);
/// How often the shutdown checks whether the last request has answered.
const DRAIN_POLL: Duration = Duration::from_millis(20);
/// Notifications the Swift helper's overlay pill consumes. Linux has no pill, so they are
/// dropped quietly instead of logged as unknown.
const PILL_NOTIFICATIONS: &[&str] = &["overlay_state", "overlay_visible", "overlay_level"];

type Running = Arc<Mutex<HashMap<u64, AbortHandle>>>;

pub(crate) fn run() -> ExitCode {
    let Some(models_dir) = models_dir_argument(std::env::args().skip(1)) else {
        log::error("usage: simple-voice-engine --models-dir <path>");
        return ExitCode::from(2);
    };
    if let Err(error) = std::fs::create_dir_all(&models_dir) {
        log::error(&format!(
            "cannot create models directory {}: {error}",
            models_dir.display()
        ));
        return ExitCode::FAILURE;
    }
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            log::error(&format!("cannot start the async runtime: {error}"));
            return ExitCode::FAILURE;
        }
    };
    log::info(&format!(
        "started (version {VERSION}, models in {})",
        models_dir.display()
    ));
    runtime.block_on(serve(Arc::new(Engine::new(models_dir))));
    log::info("stdin closed, exiting");
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
    ExitCode::SUCCESS
}

fn models_dir_argument(mut arguments: impl Iterator<Item = String>) -> Option<PathBuf> {
    while let Some(argument) = arguments.next() {
        if argument == "--models-dir" {
            return arguments
                .next()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from);
        }
    }
    None
}

async fn serve(engine: Arc<Engine>) {
    let running: Running = Arc::default();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => accept(&engine, &running, &line),
            Ok(None) => break,
            Err(error) => {
                log::error(&format!("reading stdin: {error}"));
                break;
            }
        }
    }
    // A caller that writes its requests and closes stdin (a script, `engine-linux/check.sh`)
    // still gets its answers: quick requests finish within the grace, long ones are aborted.
    let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
    while !lock(&running).is_empty() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(DRAIN_POLL).await;
    }
    let handles: Vec<AbortHandle> = lock(&running).drain().map(|(_, handle)| handle).collect();
    for handle in handles {
        handle.abort();
    }
}

fn accept(engine: &Arc<Engine>, running: &Running, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    let Ok(Value::Object(params)) = serde_json::from_str::<Value>(line) else {
        log::error(&format!(
            "ignoring a line that is not a JSON request: {}",
            preview(line)
        ));
        return;
    };
    let params = Value::Object(params);
    let cmd = params["cmd"].as_str().unwrap_or_default().to_owned();
    let Some(id) = params["id"].as_u64() else {
        if !PILL_NOTIFICATIONS.contains(&cmd.as_str()) {
            log::error(&format!("ignoring unknown notification `{cmd}`"));
        }
        return;
    };
    let output = Arc::clone(&engine.output);
    if cmd.is_empty() {
        output.failure(id, "missing `cmd`");
        return;
    }
    if cmd == "cancel" {
        let target = params["target"].as_u64();
        let handle = target.and_then(|target| lock(running).remove(&target));
        let cancelled = handle.is_some();
        if let (Some(handle), Some(target)) = (handle, target) {
            handle.abort();
            output.failure(target, "cancelled");
        }
        output.success(id, &serde_json::json!({ "cancelled": cancelled }));
        return;
    }
    // Held across the spawn so a request that finishes at once cannot remove its entry before it
    // is inserted.
    let mut tasks = lock(running);
    let task_engine = Arc::clone(engine);
    let task_running = Arc::clone(running);
    let task = tokio::spawn(async move {
        let progress = Progress::new(id, Arc::clone(&task_engine.output));
        let result = router::route(&task_engine, &cmd, &params, &progress).await;
        if lock(&task_running).remove(&id).is_none() {
            // Cancelled meanwhile; the cancel already answered for it.
            return;
        }
        match result {
            Ok(value) => task_engine.output.success(id, &value),
            Err(message) => task_engine.output.failure(id, &message),
        }
    });
    tasks.insert(id, task.abort_handle());
}

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn preview(line: &str) -> String {
    line.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> impl Iterator<Item = String> {
        values
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn reads_the_models_directory_argument() {
        assert_eq!(
            models_dir_argument(args(&["--models-dir", "/tmp/models"])),
            Some(PathBuf::from("/tmp/models"))
        );
        assert_eq!(models_dir_argument(args(&["--models-dir"])), None);
        assert_eq!(models_dir_argument(args(&["--models-dir", ""])), None);
        assert_eq!(models_dir_argument(args(&[])), None);
    }
}
