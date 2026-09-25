// FilePath: src-tauri/src/platform/engine_process.rs
//! Runs the Swift engine helper as a sidecar, wires its pipes to `EngineClient`, and restarts it
//! when it exits so a crash in Apple frameworks never takes dictation down for good.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sv_domain::{AppError, AppResult};
use sv_engine::Transport;
use tauri::async_runtime::Receiver;
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::features::{models, permissions};
use crate::state::{lock, AppState};

const SIDECAR: &str = "simple-voice-engine";
const BACKOFF: [Duration; 6] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
];
/// A helper that stayed up this long is considered healthy again and the backoff resets.
const HEALTHY_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Default)]
pub(crate) struct EngineProcess {
    transport: Mutex<Option<Arc<ChildTransport>>>,
    shutting_down: AtomicBool,
}

#[derive(Debug)]
struct ChildTransport {
    child: Mutex<Option<CommandChild>>,
}

impl Transport for ChildTransport {
    fn send_line(&self, line: String) -> AppResult<()> {
        let mut child = lock(&self.child);
        let child = child
            .as_mut()
            .ok_or_else(|| AppError::engine("The Simple Voice engine is not running"))?;
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        child.write(&bytes).map_err(AppError::engine)
    }
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(supervise(app));
}

/// Kills the helper on quit; the supervisor sees the flag and does not respawn it.
pub(crate) fn shutdown(app: &AppHandle) {
    let process = app.state::<EngineProcess>();
    process.shutting_down.store(true, Ordering::SeqCst);
    let transport = lock(&process.transport).take();
    if let Some(transport) = transport {
        if let Some(child) = lock(&transport.child).take() {
            if let Err(error) = child.kill() {
                tracing::warn!(%error, "could not stop the engine helper");
            }
        }
    }
}

async fn supervise(app: AppHandle) {
    let mut failures = 0usize;
    loop {
        let started = Instant::now();
        match spawn(&app) {
            Ok(events) => {
                run_until_exit(&app, events).await;
                app.state::<AppState>().engine.on_terminated();
                *lock(&app.state::<EngineProcess>().transport) = None;
            }
            Err(error) => tracing::error!(%error, "could not start the engine helper"),
        }
        if app
            .state::<EngineProcess>()
            .shutting_down
            .load(Ordering::SeqCst)
        {
            return;
        }
        if started.elapsed() >= HEALTHY_AFTER {
            failures = 0;
        }
        let delay = BACKOFF[failures.min(BACKOFF.len() - 1)];
        failures = failures.saturating_add(1);
        tracing::warn!(?delay, "engine helper exited; restarting");
        tokio::time::sleep(delay).await;
    }
}

fn spawn(app: &AppHandle) -> AppResult<Receiver<CommandEvent>> {
    let models_dir = sv_storage::paths::models_dir()?;
    let command = app
        .shell()
        .sidecar(SIDECAR)
        .map_err(AppError::engine)?
        .arg("--models-dir")
        .arg(models_dir);
    let (events, child) = command.spawn().map_err(AppError::engine)?;
    let transport = Arc::new(ChildTransport {
        child: Mutex::new(Some(child)),
    });
    *lock(&app.state::<EngineProcess>().transport) = Some(Arc::clone(&transport));
    app.state::<AppState>().engine.connect(transport);
    tauri::async_runtime::spawn(on_connected(app.clone()));
    Ok(events)
}

async fn run_until_exit(app: &AppHandle, mut events: Receiver<CommandEvent>) {
    while let Some(event) = events.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                for line in text.lines() {
                    let line = line.trim();
                    if !line.is_empty() {
                        app.state::<AppState>().engine.on_stdout_line(line);
                    }
                }
            }
            CommandEvent::Stderr(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                tracing::info!(target: "engine", "{}", text.trim_end());
            }
            CommandEvent::Error(error) => tracing::warn!(%error, "engine helper pipe error"),
            CommandEvent::Terminated(payload) => {
                tracing::warn!(code = ?payload.code, signal = ?payload.signal, "engine exited");
                return;
            }
            _ => {}
        }
    }
}

/// Handshake, then warm up what the next dictation needs.
async fn on_connected(app: AppHandle) {
    let state = app.state::<AppState>();
    match state.engine.ping().await {
        Ok(ping) => {
            tracing::info!(version = %ping.version, "engine helper ready");
            *lock(&state.engine_version) = Some(ping.version);
        }
        Err(error) => {
            tracing::warn!(%error, "engine helper did not answer ping");
            return;
        }
    }
    if let Err(error) = permissions::refresh(&app).await {
        tracing::debug!(%error, "permission check after engine start failed");
    }
    let model = match state.db() {
        Ok(db) => match db.settings().await {
            Ok(settings) => settings.transcription_model,
            Err(error) => {
                tracing::warn!(%error, "could not read the selected model");
                return;
            }
        },
        Err(_) => return,
    };
    models::preload(&app, &model).await;
}
