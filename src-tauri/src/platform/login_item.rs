// FilePath: src-tauri/src/platform/login_item.rs
//! Launch at login. The engine helper registers the app with `SMAppService.mainApp`, which lists
//! Simple Voice under System Settings → General → Login Items → Open at Login.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use sv_domain::{AppError, AppResult};
use sv_engine::LoginItemStatus;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// A launch this soon after the Dock started (the Dock starts when the user logs in) counts as
/// a launch at login. Generous, because login items can wait while the session settles.
const LOGIN_WINDOW: Duration = Duration::from_secs(120);

/// What `tauri-plugin-autostart` wrote before the switch to `SMAppService`, named after the app.
const LEGACY_AGENT: &str = "Library/LaunchAgents/Simple Voice.plist";

/// `SMAppService` launches the app without arguments, so a login launch is recognised by the
/// Dock having started moments earlier. A manual launch right after a Dock restart
/// (`killall Dock`) is mistaken for one and starts in the menu bar, which is harmless.
pub(crate) fn launched_at_login() -> bool {
    let output = match Command::new("/bin/ps")
        .args(["-x", "-o", "etime=,comm="])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            tracing::warn!(status = %output.status, "ps failed; assuming a manual launch");
            return false;
        }
        Err(error) => {
            tracing::warn!(%error, "could not run ps; assuming a manual launch");
            return false;
        }
    };
    dock_uptime(&String::from_utf8_lossy(&output.stdout))
        .is_some_and(|uptime| uptime <= LOGIN_WINDOW)
}

/// Registers or removes the login item. A registration macOS holds for approval is an error so
/// the setting stays off; once the user allows it, `follow_login_item` switches the setting on.
pub(crate) async fn set(app: &AppHandle, enabled: bool) -> AppResult<()> {
    let status = app
        .state::<AppState>()
        .engine
        .set_login_item(enabled)
        .await?;
    match (enabled, status) {
        (true, LoginItemStatus::Enabled) | (false, LoginItemStatus::Disabled) => Ok(()),
        (true, LoginItemStatus::RequiresApproval) => Err(AppError::Permission(
            "Allow Simple Voice in System Settings → General → Login Items, then switch this \
             on again."
                .to_owned(),
        )),
        _ => Err(AppError::Engine(format!(
            "macOS reports the login item as {status:?} after the change"
        ))),
    }
}

/// Deletes the LaunchAgent an older version registered; returns whether there was one.
pub(crate) fn remove_legacy_agent() -> bool {
    let Some(path) = legacy_agent_path() else {
        return false;
    };
    match std::fs::remove_file(&path) {
        Ok(()) => {
            tracing::info!(path = %path.display(), "removed the old launch-at-login agent");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not remove the old agent");
            false
        }
    }
}

fn legacy_agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(LEGACY_AGENT))
}

/// Finds the Dock in `ps -o etime=,comm=` output and returns how long it has been running.
fn dock_uptime(ps: &str) -> Option<Duration> {
    ps.lines().find_map(|line| {
        let (elapsed, command) = line.trim().split_once(char::is_whitespace)?;
        let command = command.trim();
        let is_dock = command == "Dock" || command.ends_with("/Dock.app/Contents/MacOS/Dock");
        if is_dock {
            parse_elapsed(elapsed)
        } else {
            None
        }
    })
}

/// Parses ps's `[[dd-]hh:]mm:ss` elapsed time.
fn parse_elapsed(elapsed: &str) -> Option<Duration> {
    let (days, clock) = match elapsed.split_once('-') {
        Some((days, clock)) => (days.parse::<u64>().ok()?, clock),
        None => (0, elapsed),
    };
    let parts = clock
        .split(':')
        .map(|part| part.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let (hours, minutes, seconds) = match parts.as_slice() {
        [minutes, seconds] => (0, *minutes, *seconds),
        [hours, minutes, seconds] => (*hours, *minutes, *seconds),
        _ => return None,
    };
    Some(Duration::from_secs(
        ((days * 24 + hours) * 60 + minutes) * 60 + seconds,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_elapsed_form() {
        assert_eq!(parse_elapsed("01:30"), Some(Duration::from_secs(90)));
        assert_eq!(parse_elapsed("02:00:05"), Some(Duration::from_secs(7205)));
        assert_eq!(
            parse_elapsed("1-00:00:01"),
            Some(Duration::from_secs(86_401))
        );
        assert_eq!(parse_elapsed("1:2:3:4"), None);
        assert_eq!(parse_elapsed("ab:cd"), None);
    }

    #[test]
    fn finds_the_dock_and_not_similar_names() {
        let ps = "   02:41 /usr/libexec/DockHelper\n  01:05 Dock\n 00:10 Finder\n";
        assert_eq!(dock_uptime(ps), Some(Duration::from_secs(65)));
        let full_path = " 1-02:00:00 /System/Library/CoreServices/Dock.app/Contents/MacOS/Dock\n";
        assert_eq!(dock_uptime(full_path), Some(Duration::from_secs(93_600)));
        assert_eq!(dock_uptime(" 00:05 DockHelper\n"), None);
    }
}
