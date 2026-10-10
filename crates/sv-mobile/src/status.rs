// FilePath: crates/sv-mobile/src/status.rs
//! What the keyboard's top bar says before speaking: the model a dictation would use, or why
//! it cannot start (MB-3). Offline, an iPhone with Apple Speech still dictates (MB-4); Android
//! has no on-device model, so offline means no dictation there.

use serde::Serialize;
use sv_domain::models::{model_name, APPLE_SPEECH, GROQ_WHISPER};
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

/// Where a dictation can be transcribed right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    pub online: bool,
    /// Apple Speech can transcribe on this device (iPhone, iOS 26 or later, model installed).
    pub on_device_speech: bool,
}

/// A missing key outranks a missing connection: it does not fix itself. Offline Apple Speech
/// needs no key, but a phone without one cannot dictate online either, so the key comes first.
pub async fn keyboard_status(db: &Db, reach: Reach) -> KeyboardStatus {
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
    } else if reach.online {
        let model = model_name(GROQ_WHISPER).unwrap_or(GROQ_WHISPER);
        (format!("{model} · online"), true)
    } else if reach.on_device_speech && settings.apple_speech_offline {
        let model = model_name(APPLE_SPEECH).unwrap_or(APPLE_SPEECH);
        (format!("{model} · offline"), true)
    } else {
        (OFFLINE.to_owned(), false)
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

        let offline = keyboard_status(&db, reach(false, true)).await;
        assert_eq!(offline.line, NO_GROQ_KEY);
        assert!(!offline.ready);

        db.set_groq_api_key(Some("test-key".to_owned()))
            .await
            .unwrap();
        let offline = keyboard_status(&db, reach(false, false)).await;
        assert_eq!(offline.line, OFFLINE);
        assert!(!offline.ready);

        let online = keyboard_status(&db, reach(true, true)).await;
        assert_eq!(online.line, "Groq Whisper · online");
        assert!(online.ready);
        assert_eq!(online.max_seconds, 300);
    }

    #[tokio::test]
    async fn offline_apple_speech_follows_its_setting() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        db.set_groq_api_key(Some("test-key".to_owned()))
            .await
            .unwrap();

        let on = keyboard_status(&db, reach(false, true)).await;
        assert_eq!(on.line, "Apple Speech · offline");
        assert!(on.ready);

        let patch = sv_domain::SettingsPatch {
            apple_speech_offline: Some(false),
            ..Default::default()
        };
        db.update_settings(patch).await.unwrap();
        let off = keyboard_status(&db, reach(false, true)).await;
        assert_eq!(off.line, OFFLINE);
        assert!(!off.ready);
    }

    fn reach(online: bool, on_device_speech: bool) -> Reach {
        Reach {
            online,
            on_device_speech,
        }
    }
}
