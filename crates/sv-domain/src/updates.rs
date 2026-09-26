// FilePath: crates/sv-domain/src/updates.rs
//! What the app knows about newer releases, and whether it is installing one. The published
//! install script does the installing; the app finds the release and starts the script.

use serde::{Deserialize, Serialize};

/// A published release, as GitHub reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// Semantic version without the tag's `v` prefix, e.g. `0.3.0`.
    pub version: String,
    /// The release page, with its notes.
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// Version of the running app.
    pub current_version: String,
    /// Newest published release, once a check has succeeded.
    pub latest: Option<Release>,
    /// `latest` is newer than the running app.
    pub update_available: bool,
    pub checking: bool,
    /// Unix ms of the last check that got an answer.
    pub checked_at: Option<i64>,
    /// Why the last check failed; cleared by the next successful one.
    pub error: Option<String>,
    /// The install script is running. A successful install quits and reopens the app.
    pub installing: bool,
    /// Why the last install failed; cleared when the next one starts.
    pub install_error: Option<String>,
}

impl UpdateStatus {
    pub fn new(current_version: String) -> Self {
        Self {
            current_version,
            latest: None,
            update_available: false,
            checking: false,
            checked_at: None,
            error: None,
            installing: false,
            install_error: None,
        }
    }
}
