// FilePath: crates/sv-mobile/src/status.rs
//! What the keyboard's top bar says before speaking: the model a dictation would use, or why
//! it cannot start (MB-3). Android has no on-device model, so offline means no dictation.

use serde::Serialize;
use sv_domain::models::{model_name, GROQ_WHISPER};
use sv_storage::Db;

pub(crate) const NO_GROQ_KEY: &str = "Add your Groq key in the Simple Voice app";
const OFFLINE: &str = "No internet · can't dictate";
const UNREADABLE: &str = "Simple Voice can't read its settings";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardStatus {
    pub line: String,
    /// Whether the mic can start a dictation now.
    pub ready: bool,
    /// The Groq key is missing: the mic opens the app to add one.
    pub needs_key: bool,
    /// The watchdog: a recording stops and is transcribed after this long (D-13).
    pub max_seconds: u32,
}

/// A missing key outranks a missing connection: it does not fix itself.
pub async fn keyboard_status(db: &Db, online: bool) -> KeyboardStatus {
    let settings = match db.settings().await {
        Ok(settings) => settings,
        Err(error) => {
            tracing::warn!(%error, "keyboard could not read settings");
            return KeyboardStatus {
                line: UNREADABLE.to_owned(),
                ready: false,
                needs_key: false,
                max_seconds: 0,
            };
        }
    };
    let has_key = db
        .groq_api_key()
        .await
        .ok()
        .flatten()
        .is_some_and(|key| !key.trim().is_empty());
    let (line, ready) = if !has_key {
        (NO_GROQ_KEY.to_owned(), false)
    } else if !online {
        (OFFLINE.to_owned(), false)
    } else {
        let model = model_name(GROQ_WHISPER).unwrap_or(GROQ_WHISPER);
        (format!("{model} · online"), true)
    };
    KeyboardStatus {
        line,
        ready,
        needs_key: !has_key,
        max_seconds: settings.max_recording_seconds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_missing_key_blocks_dictation_before_the_connection_does() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        if db.groq_api_key().await.unwrap().is_some() {
            // GROQ_API_KEY in the environment supplies a key; this case cannot be set up.
            return;
        }

        let offline = keyboard_status(&db, false).await;
        assert_eq!(offline.line, NO_GROQ_KEY);
        assert!(!offline.ready);

        db.set_groq_api_key(Some("test-key".to_owned()))
            .await
            .unwrap();
        let offline = keyboard_status(&db, false).await;
        assert_eq!(offline.line, OFFLINE);
        assert!(!offline.ready);

        let online = keyboard_status(&db, true).await;
        assert_eq!(online.line, "Groq Whisper · online");
        assert!(online.ready);
        assert_eq!(online.max_seconds, 300);
    }
}
