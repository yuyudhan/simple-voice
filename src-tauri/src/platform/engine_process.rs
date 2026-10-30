// FilePath: src-tauri/src/platform/engine_process.rs
//! Runs the Swift engine helper as a sidecar, wires its pipes to `EngineClient`, and restarts it
//! when it exits so a crash in Apple frameworks never takes dictation down for good. Settings
//! sees whether it runs (`engine_status`, the `engine-status` event) and why not, and can start
//! it at once (`restart_engine`) instead of waiting out the backoff.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sv_domain::{AppError, AppResult, EngineState, EngineStatus};
use sv_engine::Transport;
use tauri::async_runtime::Receiver;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use tokio::sync::Notify;

use crate::events;
use crate::features::{models, permissions, settings};
use crate::platform::{overlay, shortcuts};
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
/// Stderr lines kept from the running helper to explain its exit; dyld and Swift runtime
/// failures write a few lines just before the process dies.
const STDERR_TAIL: usize = 20;
/// Longest reason shown in Settings; dyld's "tried:" lists run to kilobytes.
const REASON_CHARS: usize = 200;

#[derive(Debug, Default)]
pub(crate) struct EngineProcess {
    transport: Mutex<Option<Arc<ChildTransport>>>,
    shutting_down: AtomicBool,
    status: Mutex<EngineStatus>,
    /// Set by `restart_engine`: the next start happens now and the backoff starts over.
    restart_requested: AtomicBool,
    /// Ends the supervisor's pause between restarts early.
    wake: Notify,
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

/// Escalating pause between automatic restarts, so a helper that dies at launch is retried
/// without spinning while a one-off crash comes back within a second.
#[derive(Debug, Default)]
struct Backoff {
    failures: usize,
}

impl Backoff {
    /// The pause before starting again after a helper that ran for `uptime`.
    fn after_exit(&mut self, uptime: Duration) -> Duration {
        if uptime >= HEALTHY_AFTER {
            self.failures = 0;
        }
        let delay = BACKOFF[self.failures.min(BACKOFF.len() - 1)];
        self.failures = self.failures.saturating_add(1);
        delay
    }

    fn reset(&mut self) {
        self.failures = 0;
    }
}

/// How one helper run ended: what it last wrote to stderr and how the process exited.
#[derive(Debug, Default)]
struct Exit {
    stderr: VecDeque<String>,
    code: Option<i32>,
    signal: Option<i32>,
}

impl Exit {
    fn record_stderr(&mut self, text: &str) {
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            if self.stderr.len() == STDERR_TAIL {
                self.stderr.pop_front();
            }
            self.stderr.push_back(line.to_owned());
        }
    }

    /// One line for Settings. `answered` tells a helper that never came up from one that died
    /// later, which is the difference between a broken install and a crash.
    fn reason(&self, answered: bool) -> String {
        let what = if answered {
            "The speech engine stopped"
        } else {
            "The speech engine could not start"
        };
        if let Some(detail) = self
            .stderr
            .iter()
            .rev()
            .find_map(|line| failure_detail(line))
        {
            return format!("{what}: {detail}");
        }
        match (self.code, self.signal) {
            (_, Some(signal)) => format!("{what} (signal {signal})"),
            (Some(code), None) => format!("{what} (exit code {code})"),
            (None, None) => what.to_owned(),
        }
    }
}

/// The message of a stderr line that explains a failure, without its prefix: dyld refusing to
/// load a library, a Swift runtime trap, or the helper's own error log.
fn failure_detail(line: &str) -> Option<String> {
    let message = if let Some(rest) = line.strip_prefix("dyld[") {
        rest.split_once("]: ")?.1
    } else if let Some((_, rest)) = line.split_once("Fatal error: ") {
        rest
    } else if let Some((_, rest)) = line.split_once("[error] ") {
        rest
    } else {
        return None;
    };
    let message = message.trim();
    if message.is_empty() {
        return None;
    }
    Some(clip(message))
}

fn clip(text: &str) -> String {
    match text.char_indices().nth(REASON_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(supervise(app));
}

/// Kills the helper on quit; the supervisor sees the flag and does not respawn it.
pub(crate) fn shutdown(app: &AppHandle) {
    let process = app.state::<EngineProcess>();
    process.shutting_down.store(true, Ordering::SeqCst);
    kill_child(&process);
}

/// Whether the helper runs, and why not when it does not.
#[tauri::command]
pub(crate) fn engine_status(process: State<'_, EngineProcess>) -> EngineStatus {
    lock(&process.status).clone()
}

/// Starts the helper now: a running one is stopped first, a pause between automatic restarts
/// ends, and the backoff starts over.
#[tauri::command]
pub(crate) fn restart_engine(app: AppHandle) -> EngineStatus {
    let process = app.state::<EngineProcess>();
    process.restart_requested.store(true, Ordering::SeqCst);
    // Without a child the supervisor is pausing (or about to); with one, its exit wakes it.
    if !kill_child(&process) {
        process.wake.notify_one();
    }
    tracing::info!("engine helper restart requested");
    set_status(&app, EngineStatus::starting());
    EngineStatus::starting()
}

/// Kills the current helper, if there is one; true when one was running.
fn kill_child(process: &EngineProcess) -> bool {
    let transport = lock(&process.transport).take();
    let Some(child) = transport.and_then(|transport| lock(&transport.child).take()) else {
        return false;
    };
    if let Err(error) = child.kill() {
        tracing::warn!(%error, "could not stop the engine helper");
    }
    true
}

fn set_status(app: &AppHandle, status: EngineStatus) {
    let previous = std::mem::replace(
        &mut *lock(&app.state::<EngineProcess>().status),
        status.clone(),
    );
    if previous == status {
        return;
    }
    let running = status.state == EngineState::Running;
    events::emit(app, events::ENGINE_STATUS, status);
    // Model readiness comes from the helper; what Settings showed while it was down is stale.
    if running {
        events::emit(app, events::MODELS_CHANGED, ());
    }
}

async fn supervise(app: AppHandle) {
    let process = app.state::<EngineProcess>();
    let mut backoff = Backoff::default();
    loop {
        let started = Instant::now();
        set_status(&app, EngineStatus::starting());
        let reason = match spawn(&app) {
            Ok(events) => {
                let exit = run_until_exit(&app, events).await;
                app.state::<AppState>().engine.on_terminated();
                *lock(&process.transport) = None;
                let answered = lock(&process.status).state == EngineState::Running;
                exit.reason(answered)
            }
            Err(error) => format!("The speech engine could not be launched: {error}"),
        };
        if process.shutting_down.load(Ordering::SeqCst) {
            return;
        }
        if process.restart_requested.swap(false, Ordering::SeqCst) {
            backoff.reset();
            continue;
        }
        tracing::warn!(%reason, "engine helper is down");
        set_status(&app, EngineStatus::down(reason));
        let delay = backoff.after_exit(started.elapsed());
        tracing::warn!(?delay, "restarting the engine helper");
        tokio::select! {
            () = tokio::time::sleep(delay) => {}
            () = process.wake.notified() => {}
        }
        if process.restart_requested.swap(false, Ordering::SeqCst) {
            backoff.reset();
        }
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

async fn run_until_exit(app: &AppHandle, mut events: Receiver<CommandEvent>) -> Exit {
    let mut exit = Exit::default();
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
                exit.record_stderr(&text);
            }
            CommandEvent::Error(error) => tracing::warn!(%error, "engine helper pipe error"),
            CommandEvent::Terminated(payload) => {
                tracing::warn!(code = ?payload.code, signal = ?payload.signal, "engine exited");
                exit.code = payload.code;
                exit.signal = payload.signal;
                break;
            }
            _ => {}
        }
    }
    exit
}

/// Handshake, then warm up what the next dictation needs.
async fn on_connected(app: AppHandle) {
    let state = app.state::<AppState>();
    match state.engine.ping().await {
        Ok(ping) => {
            tracing::info!(version = %ping.version, "engine helper ready");
            *lock(&state.engine_version) = Some(ping.version);
            set_status(&app, EngineStatus::running());
        }
        Err(error) => {
            tracing::warn!(%error, "engine helper did not answer ping");
            // A helper that exited is reported by the supervisor with its own reason.
            if lock(&app.state::<EngineProcess>().transport).is_some() {
                set_status(
                    &app,
                    EngineStatus::down(format!("The speech engine does not answer: {error}")),
                );
            }
            return;
        }
    }
    shortcuts::sync_fn_key(&app);
    overlay::resync(&app);
    if let Err(error) = permissions::refresh(&app).await {
        tracing::debug!(%error, "permission check after engine start failed");
    }
    settings::follow_login_item(&app).await;
    let settings = match state.db() {
        Ok(db) => match db.settings().await {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(%error, "could not read the selected model");
                return;
            }
        },
        Err(_) => return,
    };
    models::preload_selected(&app, &settings).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exit_with(lines: &[&str], code: Option<i32>, signal: Option<i32>) -> Exit {
        let mut exit = Exit {
            code,
            signal,
            ..Exit::default()
        };
        for line in lines {
            exit.record_stderr(line);
        }
        exit
    }

    #[test]
    fn backoff_escalates_and_caps_at_thirty_seconds() {
        let mut backoff = Backoff::default();
        let quick = Duration::from_millis(50);
        let delays: Vec<u64> = (0..8)
            .map(|_| backoff.after_exit(quick).as_secs())
            .collect();
        assert_eq!(delays, vec![1, 2, 5, 10, 20, 30, 30, 30]);
    }

    #[test]
    fn a_helper_that_stayed_up_restarts_after_the_shortest_pause() {
        let mut backoff = Backoff::default();
        for _ in 0..5 {
            backoff.after_exit(Duration::ZERO);
        }
        assert_eq!(backoff.after_exit(HEALTHY_AFTER), Duration::from_secs(1));
        assert_eq!(backoff.after_exit(Duration::ZERO), Duration::from_secs(2));
    }

    #[test]
    fn a_manual_restart_starts_the_backoff_over() {
        let mut backoff = Backoff::default();
        for _ in 0..10 {
            backoff.after_exit(Duration::ZERO);
        }
        backoff.reset();
        assert_eq!(backoff.after_exit(Duration::ZERO), Duration::from_secs(1));
    }

    #[test]
    fn a_dyld_failure_names_the_missing_library() {
        let exit = exit_with(
            &["dyld[18140]: Library not loaded: @rpath/llama.framework/Versions/Current/llama\n  \
               Referenced from: <7BD8> /Applications/Simple Voice.app/Contents/MacOS/engine\n  \
               Reason: tried: '/usr/lib/swift/llama.framework' (no such file)"],
            None,
            Some(6),
        );
        assert_eq!(
            exit.reason(false),
            "The speech engine could not start: Library not loaded: \
             @rpath/llama.framework/Versions/Current/llama"
        );
    }

    #[test]
    fn a_swift_trap_or_logged_error_explains_a_crash() {
        let trap = exit_with(
            &[
                "simple-voice-engine [info] started (version 0.1.1)",
                "Audio.swift:42: Fatal error: Index out of range",
            ],
            None,
            Some(5),
        );
        assert_eq!(
            trap.reason(true),
            "The speech engine stopped: Index out of range"
        );
        let logged = exit_with(
            &["simple-voice-engine [error] could not open the models folder"],
            Some(1),
            None,
        );
        assert_eq!(
            logged.reason(false),
            "The speech engine could not start: could not open the models folder"
        );
    }

    #[test]
    fn without_an_explanation_the_exit_status_is_the_reason() {
        let info_only = ["simple-voice-engine [info] started (version 0.1.1)"];
        assert_eq!(
            exit_with(&info_only, None, Some(9)).reason(true),
            "The speech engine stopped (signal 9)"
        );
        assert_eq!(
            exit_with(&info_only, Some(3), None).reason(true),
            "The speech engine stopped (exit code 3)"
        );
        assert_eq!(
            exit_with(&[], None, None).reason(false),
            "The speech engine could not start"
        );
    }

    #[test]
    fn stderr_keeps_only_the_last_lines_and_long_reasons_are_clipped() {
        let mut exit = Exit::default();
        for n in 0..(STDERR_TAIL + 5) {
            exit.record_stderr(&format!("line {n}\n\n"));
        }
        assert_eq!(exit.stderr.len(), STDERR_TAIL);
        assert_eq!(exit.stderr.front().map(String::as_str), Some("line 5"));

        let long = format!("dyld[1]: {}", "x".repeat(REASON_CHARS + 50));
        let detail = failure_detail(&long).unwrap_or_default();
        assert_eq!(detail.chars().count(), REASON_CHARS + 1);
        assert!(detail.ends_with('…'));
    }

    #[test]
    fn status_serializes_for_the_ui() {
        let down = serde_json::to_value(EngineStatus::down("gone".to_owned())).unwrap_or_default();
        assert_eq!(
            down,
            serde_json::json!({ "state": "down", "reason": "gone" })
        );
        let running = serde_json::to_value(EngineStatus::running()).unwrap_or_default();
        assert_eq!(
            running,
            serde_json::json!({ "state": "running", "reason": null })
        );
    }
}
