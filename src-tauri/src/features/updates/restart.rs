// FilePath: src-tauri/src/features/updates/restart.rs
//! Finishing updates. Install scripts before the `pgrep -a` fix could not quit the app that
//! started them, so they replaced the bundle under a running app, which kept running the old
//! build: the app now notices a newer version on disk and restarts into it when asked. It also
//! notices the first launch after an update, so the UI can say the update worked.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use semver::Version;
use sv_domain::{AppError, AppResult, UpdateStatus};
use tauri::{AppHandle, Manager};

use crate::events;
use crate::state::{lock, AppState};

const PLIST_BUDDY: &str = "/usr/libexec/PlistBuddy";
/// Backgrounds a waiter that opens the bundle through LaunchServices once this process is gone.
/// `-n` launches even while LaunchServices still counts the old process as running, which would
/// otherwise turn the `open` into a no-op activation.
const RELAUNCHER: &str = "(while /bin/kill -0 \"$1\" 2>/dev/null; do /bin/sleep 0.2; done; \
    /usr/bin/open -n \"$2\") </dev/null >/dev/null 2>&1 &";

/// The `.app` bundle this executable runs from; `None` outside one, as under `just dev`.
pub(crate) fn bundle_path() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let in_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    in_bundle.then(|| bundle.to_path_buf())
}

/// Sets `installed_version` to the bundle's version when it is newer than the running one.
/// Returns whether the status changed.
pub(crate) fn refresh_installed(app: &AppHandle) -> bool {
    let current = &app.package_info().version;
    let installed = bundle_path()
        .and_then(|bundle| bundle_version(&bundle))
        .filter(|version| version > current)
        .map(|version| version.to_string());
    let state = app.state::<AppState>();
    let mut status = lock(&state.updates);
    if status.installed_version == installed {
        return false;
    }
    if let Some(version) = &installed {
        tracing::info!(%version, %current, "a newer version is installed; restart to run it");
    }
    status.installed_version = installed;
    true
}

/// Restarting runs the installed build, which is all the update needs when that build is at
/// least the latest release (or no release is known); an older one still needs the script.
pub(crate) fn restart_finishes_update(status: &UpdateStatus) -> bool {
    let Some(installed) = status.installed_version.as_deref().and_then(parse) else {
        return false;
    };
    match status.latest.as_ref() {
        None => true,
        Some(latest) => parse(&latest.version).is_none_or(|latest| installed >= latest),
    }
}

/// Quits the app and opens `bundle` again once this process has exited.
pub(crate) fn relaunch(app: &AppHandle, bundle: &Path) -> AppResult<()> {
    let status = Command::new("/bin/sh")
        .args(["-c", RELAUNCHER, "simple-voice-relaunch"])
        .arg(std::process::id().to_string())
        .arg(bundle)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Signals aimed at the app's process group must not reach the waiter.
        .process_group(0)
        .status()
        .map_err(|error| AppError::other(format!("Could not restart Simple Voice: {error}")))?;
    if !status.success() {
        return Err(AppError::other(format!(
            "Could not restart Simple Voice ({status})"
        )));
    }
    tracing::info!(bundle = %bundle.display(), "restarting to finish the update");
    app.exit(0);
    Ok(())
}

/// Remembers this launch's version; when an older one ran last, the update just finished and
/// `updated_from` tells the UI so.
pub(crate) async fn record_launch(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Ok(db) = state.db() else {
        return;
    };
    let current = app.package_info().version.clone();
    let previous = match db.record_run_version(&current.to_string()).await {
        Ok(previous) => previous,
        Err(error) => {
            tracing::warn!(%error, "could not record the running version");
            return;
        }
    };
    let Some(from) = previous
        .as_deref()
        .and_then(parse)
        .filter(|from| *from < current)
    else {
        return;
    };
    tracing::info!(%from, %current, "updated");
    let status = {
        let mut status = lock(&state.updates);
        status.updated_from = Some(from.to_string());
        status.clone()
    };
    events::emit(app, events::UPDATE_STATUS, status);
}

/// Hides the "updated" notice.
#[tauri::command]
pub(crate) async fn dismiss_update_notice(app: AppHandle) -> AppResult<UpdateStatus> {
    let state = app.state::<AppState>();
    let status = {
        let mut status = lock(&state.updates);
        status.updated_from = None;
        status.clone()
    };
    events::emit(&app, events::UPDATE_STATUS, status.clone());
    Ok(status)
}

fn bundle_version(bundle: &Path) -> Option<Version> {
    let output = Command::new(PLIST_BUDDY)
        .args(["-c", "Print :CFBundleShortVersionString"])
        .arg(bundle.join("Contents/Info.plist"))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
        .and_then(|text| parse(&text))
}

fn parse(version: &str) -> Option<Version> {
    Version::parse(version.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_domain::Release;

    fn status(installed: Option<&str>, latest: Option<&str>) -> UpdateStatus {
        let mut status = UpdateStatus::new("0.0.8".to_owned());
        status.installed_version = installed.map(str::to_owned);
        status.latest = latest.map(|version| Release {
            version: version.to_owned(),
            url: String::new(),
        });
        status
    }

    #[test]
    fn restarts_when_the_installed_build_is_the_latest_or_newer() {
        assert!(restart_finishes_update(&status(
            Some("0.0.9"),
            Some("0.0.9")
        )));
        assert!(restart_finishes_update(&status(
            Some("0.0.10"),
            Some("0.0.9")
        )));
        assert!(restart_finishes_update(&status(Some("0.0.9"), None)));
    }

    #[test]
    fn runs_the_script_when_nothing_newer_is_installed_or_a_later_release_is_out() {
        assert!(!restart_finishes_update(&status(None, Some("0.0.9"))));
        assert!(!restart_finishes_update(&status(
            Some("0.0.9"),
            Some("0.0.10")
        )));
    }
}
