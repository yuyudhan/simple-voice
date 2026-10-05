// FilePath: mobile/src-tauri/src/state.rs
//! Process-wide state shared by every command.

use sv_domain::{AppError, AppResult};
use sv_storage::Db;

#[derive(Debug)]
pub(crate) struct AppState {
    /// A database that failed to open keeps its error so every screen reports the same message
    /// instead of the app refusing to start.
    db: Result<Db, AppError>,
    /// HTTPS for Groq; an error here is kept the same way.
    http: Result<reqwest::Client, AppError>,
}

impl AppState {
    pub(crate) fn new(db: Result<Db, AppError>, http: Result<reqwest::Client, AppError>) -> Self {
        Self { db, http }
    }

    pub(crate) fn db(&self) -> AppResult<&Db> {
        self.db.as_ref().map_err(Clone::clone)
    }

    pub(crate) fn http(&self) -> AppResult<&reqwest::Client> {
        self.http.as_ref().map_err(Clone::clone)
    }
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
