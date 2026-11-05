// FilePath: src-tauri/src/platform/output_mute/mod.rs
//! The platform boundary for muting system output while a dictation records, so every app and
//! browser tab is silent at once, and for unmuting exactly the device that was muted. The engine
//! helper implements it on macOS (Core Audio) and Linux (the Linux helper speaks the same
//! protocol, e.g. `pactl set-sink-mute`); on any other platform every call reports that it is not
//! supported. What a recording muted and how it is undone lives in `session`, the same for every
//! platform.

mod session;

use std::future::Future;
use std::path::PathBuf;

use sv_domain::{AppResult, MutedOutput};
use tauri::{AppHandle, Manager};

pub(crate) use self::session::{Muting, PendingRecovery};

/// Where a recording keeps what it muted until it is unmuted (crash and quit recovery).
const MARKER_FILE: &str = "muted-output.json";

pub(crate) trait OutputControl: Clone + Send + Sync + 'static {
    /// Mutes the default output device; `None` when it was muted already.
    fn mute_output(&self) -> impl Future<Output = AppResult<Option<MutedOutput>>> + Send;
    /// Unmutes the device `muted` names, restoring its volume when that is how it was muted;
    /// false when the device is gone.
    fn unmute_output(&self, muted: MutedOutput) -> impl Future<Output = AppResult<bool>> + Send;
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) type Backend = sv_engine::EngineClient;

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn backend(app: &AppHandle) -> Backend {
    app.state::<crate::state::AppState>().engine.clone()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl OutputControl for sv_engine::EngineClient {
    async fn mute_output(&self) -> AppResult<Option<MutedOutput>> {
        sv_engine::EngineClient::mute_output(self).await
    }

    async fn unmute_output(&self, muted: MutedOutput) -> AppResult<bool> {
        sv_engine::EngineClient::unmute_output(self, &muted).await
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) type Backend = Unsupported;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn backend(_app: &AppHandle) -> Backend {
    Unsupported
}

/// A platform without output control: recordings change nothing and say why in the log.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Unsupported;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl OutputControl for Unsupported {
    async fn mute_output(&self) -> AppResult<Option<MutedOutput>> {
        Err(unsupported())
    }

    async fn unmute_output(&self, _muted: MutedOutput) -> AppResult<bool> {
        Err(unsupported())
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn unsupported() -> sv_domain::AppError {
    sv_domain::AppError::other("Muting the speakers is not supported on this platform")
}

/// `None` when the data directory is unavailable; recordings then mute without crash recovery.
pub(crate) fn marker_path() -> Option<PathBuf> {
    match sv_storage::paths::data_dir() {
        Ok(dir) => Some(dir.join(MARKER_FILE)),
        Err(error) => {
            tracing::warn!(%error, "no data directory for the mute marker");
            None
        }
    }
}

/// Unmutes what a recording cut off by a crash or quit left muted. Runs once the helper answers,
/// since it does the unmuting.
pub(crate) async fn recover(app: &AppHandle) {
    let pending = app.state::<PendingRecovery>();
    pending.apply(&backend(app)).await;
}
