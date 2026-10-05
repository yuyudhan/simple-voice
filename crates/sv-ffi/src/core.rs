// FilePath: crates/sv-ffi/src/core.rs
//! Process-wide state for the keyboard: a runtime, the shared database and an HTTPS client,
//! created on first use and kept for the life of the keyboard process. Every answer is JSON for
//! Kotlin's `org.json`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use sv_domain::{AppError, AppResult};
use sv_mobile::{Delivery, Dictation, KeyboardStatus, Recording};
use sv_storage::Db;
use tokio::runtime::Runtime;

#[derive(Debug)]
struct Core {
    runtime: Runtime,
    db: Db,
    http: reqwest::Client,
}

static CORE: OnceLock<Core> = OnceLock::new();

const STARTING: &str = "Simple Voice is still starting";

/// Opens the database the app uses, in the app's private data directory. Repeated calls after
/// a successful one do nothing.
pub(crate) fn init(data_dir: &Path) -> AppResult<()> {
    if CORE.get().is_some() {
        return Ok(());
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(AppError::other)?;
    let db = runtime.block_on(Db::open_at(data_dir))?;
    let http = sv_mobile::http_client()?;
    // A keyboard instance racing another init keeps whichever finished first; both opened the
    // same file, so dropping the loser's handle is harmless.
    let _ = CORE.set(Core { runtime, db, http });
    Ok(())
}

pub(crate) fn status(online: bool) -> String {
    match CORE.get() {
        Some(core) => json(
            &core
                .runtime
                .block_on(sv_mobile::keyboard_status(&core.db, online)),
        ),
        None => json(&KeyboardStatus {
            line: STARTING.to_owned(),
            ready: false,
            needs_key: false,
            max_seconds: 0,
        }),
    }
}

pub(crate) fn dictate(
    wav_path: PathBuf,
    duration_ms: i64,
    app_name: Option<String>,
    app_id: Option<String>,
) -> String {
    let Some(core) = CORE.get() else {
        return not_started();
    };
    let recording = Recording {
        wav_path,
        duration_ms,
        app_name,
        app_id,
    };
    json(
        &core
            .runtime
            .block_on(sv_mobile::dictate(&core.db, &core.http, recording)),
    )
}

/// Retries a failed dictation for the cursor; the answer has the same shape as a dictation.
pub(crate) fn retry(id: i64) -> String {
    let Some(core) = CORE.get() else {
        return not_started();
    };
    let retried = core.runtime.block_on(sv_mobile::retry(
        &core.db,
        &core.http,
        id,
        Delivery::Inserted,
    ));
    json(&match retried {
        Ok((entry, text)) => Dictation::Text {
            id: entry.id,
            text,
            unformatted: false,
        },
        Err(error) => Dictation::Failed {
            id: Some(id),
            message: error.to_string(),
        },
    })
}

/// Records that the text of dictation `id` never reached its field.
pub(crate) fn mark_not_inserted(id: i64, reason: &str) {
    if let Some(core) = CORE.get() {
        if let Err(error) = core
            .runtime
            .block_on(core.db.mark_history_not_pasted(id, reason))
        {
            tracing::warn!(%error, id, "could not mark the dictation as not pasted");
        }
    }
}

fn not_started() -> String {
    json(&Dictation::Failed {
        id: None,
        message: STARTING.to_owned(),
    })
}

fn json(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        tracing::error!(%error, "could not encode an answer for the keyboard");
        r#"{"kind":"failed","message":"Simple Voice could not answer the keyboard"}"#.to_owned()
    })
}
