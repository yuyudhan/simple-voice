// FilePath: src-tauri/src/state.rs
//! Process-wide state managed by Tauri and shared by every command, the dictation coordinator,
//! the engine supervisor and the pollers.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use sv_audio::{Cue, CuePlayer};
use sv_domain::{AppError, AppResult, Permissions, Settings, SoundTheme, UpdateStatus};
use sv_engine::EngineClient;
use sv_storage::Db;

use crate::features::dictation::DictationHandle;

#[derive(Debug)]
pub(crate) struct AppState {
    /// A database that failed to open keeps its error so every command reports the same
    /// message instead of the app refusing to start.
    db: Result<Db, AppError>,
    pub(crate) http: reqwest::Client,
    pub(crate) engine: EngineClient,
    /// `None` when the Mac has no usable audio output; dictation still works without cues.
    cues: Option<CuePlayer>,
    pub(crate) dictation: DictationHandle,
    /// Model id → download fraction for downloads in flight.
    pub(crate) downloads: Mutex<HashMap<String, f32>>,
    /// Last permissions reported to the UI, so `permissions-changed` fires only on change.
    pub(crate) permissions: Mutex<Option<Permissions>>,
    pub(crate) engine_version: Mutex<Option<String>>,
    pub(crate) updates: Mutex<UpdateStatus>,
}

impl AppState {
    pub(crate) fn new(
        db: Result<Db, AppError>,
        dictation: DictationHandle,
        app_version: String,
    ) -> Self {
        let cues = match CuePlayer::new() {
            Ok(player) => Some(player),
            Err(error) => {
                tracing::warn!(%error, "audio output unavailable; cues disabled");
                None
            }
        };
        Self {
            db,
            http: reqwest::Client::new(),
            engine: EngineClient::new(),
            cues,
            dictation,
            downloads: Mutex::new(HashMap::new()),
            permissions: Mutex::new(None),
            engine_version: Mutex::new(None),
            updates: Mutex::new(UpdateStatus::new(app_version)),
        }
    }

    pub(crate) fn db(&self) -> AppResult<&Db> {
        self.db.as_ref().map_err(Clone::clone)
    }

    /// Plays a cue when the user has sounds on.
    pub(crate) fn play(&self, settings: &Settings, cue: Cue) {
        if settings.sounds {
            self.play_theme(settings.sound_theme, cue, settings.sound_volume);
        }
    }

    /// Plays a cue regardless of the sounds toggle (sound previews).
    pub(crate) fn play_theme(&self, theme: SoundTheme, cue: Cue, volume: f32) {
        if let Some(player) = &self.cues {
            player.play(theme, cue, volume);
        }
    }
}

/// Locks a mutex, recovering the data if a panicking thread poisoned it: every guarded value
/// here stays consistent between statements, so the data is still valid.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
