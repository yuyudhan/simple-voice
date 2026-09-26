// FilePath: src-tauri/src/features/updates/install.rs
//! Installs an available update by running the published install script (`scripts/install.sh`)
//! exactly as the README's curl command does. The script quits the app, replaces it and reopens
//! it, so it has to outlive the app: it runs in its own process group and writes to a log file,
//! never to a pipe that the app's exit would close under it.

use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};

use sv_domain::{AppError, AppResult, UpdateStatus};
use tauri::{AppHandle, Manager};

use super::REPOSITORY;
use crate::events;
use crate::state::{lock, AppState};

/// In the data directory; replaced by every install.
const LOG_FILE: &str = "update.log";
/// Downloads the script and pipes it into bash, exactly like the README's install command. The
/// URL arrives as `$1`, so it is never parsed as shell.
const RUNNER: &str = "set -o pipefail; /usr/bin/curl -fsSL \"$1\" | /bin/bash";
/// Every line the script itself prints starts with this.
const SCRIPT_PREFIX: &str = "simple-voice:";

#[tauri::command]
pub(crate) async fn install_update(app: AppHandle) -> AppResult<UpdateStatus> {
    let state = app.state::<AppState>();
    {
        let mut status = lock(&state.updates);
        if status.installing {
            return Ok(status.clone());
        }
        if !status.update_available {
            return Err(AppError::invalid("No update is available to install"));
        }
        status.installing = true;
        status.install_error = None;
    }

    let log = match sv_storage::paths::data_dir() {
        Ok(dir) => dir.join(LOG_FILE),
        Err(error) => return Err(abandon(&app, error)),
    };
    let child = match sv_cloud::install_script_url(REPOSITORY).and_then(|url| spawn(&url, &log)) {
        Ok(child) => child,
        Err(error) => return Err(abandon(&app, error)),
    };
    tracing::info!(pid = child.id(), log = %log.display(), "update install started");

    let status = lock(&state.updates).clone();
    events::emit(&app, events::UPDATE_STATUS, status.clone());
    tauri::async_runtime::spawn_blocking(move || finish(&app, child, &log));
    Ok(status)
}

fn spawn(script_url: &str, log: &Path) -> AppResult<Child> {
    let output = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(log)
        .map_err(AppError::io)?;
    let errors = output.try_clone().map_err(AppError::io)?;
    Command::new("/bin/bash")
        .args(["-c", RUNNER, "simple-voice-update", script_url])
        .stdin(Stdio::null())
        .stdout(output)
        .stderr(errors)
        // Signals aimed at the app's process group (Ctrl-C under `just dev`) must not reach the
        // installer halfway through replacing the app.
        .process_group(0)
        .spawn()
        .map_err(|error| AppError::other(format!("Could not start the installer: {error}")))
}

/// Starting the installer failed; the status goes back to idle and the error goes to the caller.
fn abandon(app: &AppHandle, error: AppError) -> AppError {
    lock(&app.state::<AppState>().updates).installing = false;
    error
}

/// Reached only when the script ends before the app quits: it failed, or it found nothing to
/// replace (the running build is not the one in /Applications, as under `just dev`).
fn finish(app: &AppHandle, mut child: Child, log: &Path) {
    let error = match child.wait() {
        Ok(exit) if exit.success() => None,
        Ok(exit) => {
            let output = std::fs::read_to_string(log).unwrap_or_default();
            let reason = failure_reason(&output)
                .unwrap_or_else(|| format!("The installer stopped ({exit})"));
            Some(format!("{reason}. Details are in {}.", log.display()))
        }
        Err(error) => Some(format!("Could not follow the installer: {error}")),
    };
    match &error {
        Some(error) => tracing::warn!(%error, "update install failed"),
        None => tracing::info!("update install finished without quitting the app"),
    }

    let state = app.state::<AppState>();
    let status = {
        let mut status = lock(&state.updates);
        status.installing = false;
        status.install_error = error;
        status.clone()
    };
    events::emit(app, events::UPDATE_STATUS, status);
}

/// The script's last own message (it prints its reason just before failing), else the last line
/// anyone wrote, such as curl's error when the script itself could not be downloaded.
fn failure_reason(output: &str) -> Option<String> {
    let lines: Vec<&str> = output
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let own = lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix(SCRIPT_PREFIX));
    own.or_else(|| lines.last().copied())
        .map(|line| line.trim().trim_end_matches('.').to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_scripts_last_message_over_later_tool_output() {
        let output = "simple-voice: downloading Simple Voice 0.0.4\n\
            ######## 100.0%\r\n\
            simple-voice: checksum mismatch for Simple-Voice_0.0.4_aarch64.zip; nothing was \
            installed\n\
            rm: /tmp/x: Permission denied\n";
        assert_eq!(
            failure_reason(output).as_deref(),
            Some("checksum mismatch for Simple-Voice_0.0.4_aarch64.zip; nothing was installed")
        );
    }

    #[test]
    fn falls_back_to_the_last_line_when_the_script_never_ran() {
        let output = "curl: (6) Could not resolve host: github.com\n\n";
        assert_eq!(
            failure_reason(output).as_deref(),
            Some("curl: (6) Could not resolve host: github.com")
        );
        assert_eq!(failure_reason(" \n\r\n"), None);
    }
}
