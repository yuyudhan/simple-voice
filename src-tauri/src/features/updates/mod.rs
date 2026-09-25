// FilePath: src-tauri/src/features/updates/mod.rs
//! Update notices. Homebrew installs every version (requirement P-5), so the app never downloads
//! or replaces itself: it asks GitHub for the latest release, compares it with its own version,
//! and tells the UI (`update-status`) and the tray.

use std::time::Duration;

use sv_domain::{AppResult, Release, UpdateStatus};
use tauri::{AppHandle, Manager, State};

use crate::events;
use crate::platform::{tray, windows};
use crate::state::{lock, now_ms, AppState};

/// The workspace `repository` field, so a fork checks its own releases.
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
/// Leaves launch (engine start, model preload) alone before the first request.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
/// How often the checker wakes. Monotonic sleeps pause while the Mac sleeps, so the daily
/// interval is measured against the wall clock on each wake instead of one long sleep.
const TICK: Duration = Duration::from_secs(60 * 60);
const CHECK_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;

#[tauri::command]
pub(crate) async fn get_update_status(state: State<'_, AppState>) -> AppResult<UpdateStatus> {
    Ok(lock(&state.updates).clone())
}

/// Checks now, whatever the automatic-check setting says. A failed check is reported in the
/// returned status (`error`), not as a command error.
#[tauri::command]
pub(crate) async fn check_for_updates(app: AppHandle) -> AppResult<UpdateStatus> {
    Ok(check(&app).await)
}

/// Shows the main window on the update details (tray and app menu). Without a known update it
/// also checks, so the item doubles as "Check for Updates…".
pub(crate) fn open(app: &AppHandle) {
    windows::show_main(app);
    events::emit_to(app, windows::MAIN, events::NAVIGATE, "updates");
    if !lock(&app.state::<AppState>().updates).update_available {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            check(&app).await;
        });
    }
}

/// Checks shortly after launch, then once a day while automatic checks are on. A failed check
/// leaves `checked_at` alone, so it is retried on the next tick.
pub(crate) fn spawn_checker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            if automatic_check_due(&app).await {
                check(&app).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn automatic_check_due(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    // An unreadable database may hide a user's "off", so it counts as off.
    let enabled = match state.db() {
        Ok(db) => match db.settings().await {
            Ok(settings) => settings.check_for_updates,
            Err(error) => {
                tracing::warn!(%error, "could not read settings; skipping the update check");
                false
            }
        },
        Err(_) => false,
    };
    let checked_at = lock(&state.updates).checked_at;
    enabled && checked_at.is_none_or(|at| now_ms() - at >= CHECK_INTERVAL_MS)
}

async fn check(app: &AppHandle) -> UpdateStatus {
    let state = app.state::<AppState>();
    let started = {
        let mut status = lock(&state.updates);
        if status.checking {
            return status.clone();
        }
        status.checking = true;
        status.clone()
    };
    events::emit(app, events::UPDATE_STATUS, started);

    let current = app.package_info().version.clone();
    let result = fetch(&state.http, &current).await;
    let status = {
        let mut status = lock(&state.updates);
        status.checking = false;
        match result {
            Ok(release) => {
                status.update_available = release.version > current;
                tracing::info!(
                    latest = %release.version,
                    %current,
                    available = status.update_available,
                    "update check done"
                );
                status.latest = Some(Release {
                    version: release.version.to_string(),
                    url: release.url,
                });
                status.checked_at = Some(now_ms());
                status.error = None;
            }
            Err(error) => {
                tracing::info!(%error, "update check failed");
                status.error = Some(error.to_string());
            }
        }
        status.clone()
    };

    let available = status
        .latest
        .as_ref()
        .filter(|_| status.update_available)
        .map(|release| release.version.as_str());
    tray::set_update(app, available);
    events::emit(app, events::UPDATE_STATUS, status.clone());
    status
}

async fn fetch(
    client: &reqwest::Client,
    current: &semver::Version,
) -> AppResult<sv_cloud::LatestRelease> {
    let url = sv_cloud::latest_release_url(REPOSITORY)?;
    sv_cloud::latest_release(client, &url, &format!("Simple-Voice/{current}")).await
}
