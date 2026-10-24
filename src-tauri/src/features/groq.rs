// FilePath: src-tauri/src/features/groq.rs
//! The optional Groq provider's key and requests. The user's own API key is kept in the login
//! Keychain (a generic password owned by this app, never in the database, logs or settings),
//! read once per launch and cached in memory. Settings saves a key only after Groq accepts it.

use std::fmt;
use std::time::Duration;

use serde::Serialize;
use sv_domain::{AppError, AppResult};
use sv_text::PolishPrompt;
use tauri::State;

use crate::features::llm::Reply;
use crate::state::AppState;

/// One Keychain item per app identity, so the development build never reads the release's key.
const KEYCHAIN_SERVICE: &str = if cfg!(debug_assertions) {
    "dev.yuyudhan.simplevoice.dev.groq"
} else {
    "dev.yuyudhan.simplevoice.groq"
};
const KEYCHAIN_ACCOUNT: &str = "groq-api-key";
/// `errSecItemNotFound`: no key saved.
const ITEM_NOT_FOUND: i32 = -25_300;

pub(crate) const NO_KEY: &str = "Add your Groq API key in Settings to use Groq.";

/// The key as last read from or written to the Keychain: `None` until the first read,
/// `Some(None)` when no key is saved. Its `Debug` never prints the key.
#[derive(Default)]
pub(crate) struct KeyCache(tokio::sync::Mutex<Option<Option<String>>>);

impl fmt::Debug for KeyCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyCache(<redacted>)")
    }
}

/// Whether a Groq key is saved; the key itself never leaves the backend.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GroqKeyStatus {
    saved: bool,
}

/// The saved key, `None` when there is none. The first call reads the Keychain; later calls
/// use the cached copy.
pub(crate) async fn key(state: &AppState) -> AppResult<Option<String>> {
    let mut cached = state.groq_key.0.lock().await;
    if let Some(known) = cached.as_ref() {
        return Ok(known.clone());
    }
    let read = blocking(read_keychain).await?;
    *cached = Some(read.clone());
    Ok(read)
}

/// One Groq chat completion with the saved key.
pub(crate) async fn chat(
    state: &AppState,
    model: &str,
    prompt: &PolishPrompt,
    max_tokens: u32,
    limit: Duration,
) -> Result<Reply, String> {
    let key = key(state)
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| NO_KEY.to_owned())?;
    let chat = sv_cloud::GroqChat { key: &key, model };
    let reply = sv_cloud::chat_completion(&state.http, &chat, prompt, max_tokens, limit).await?;
    Ok(Reply {
        text: reply.text,
        finished: reply.finished,
    })
}

#[tauri::command]
pub(crate) async fn groq_key_status(state: State<'_, AppState>) -> AppResult<GroqKeyStatus> {
    Ok(GroqKeyStatus {
        saved: key(&state).await?.is_some(),
    })
}

/// Checks `key` with Groq, then saves it in the Keychain, replacing any earlier one.
#[tauri::command]
pub(crate) async fn save_groq_api_key(
    state: State<'_, AppState>,
    key: String,
) -> AppResult<GroqKeyStatus> {
    let key = normalize_key(&key).ok_or_else(|| AppError::invalid("Enter a Groq API key"))?;
    sv_cloud::groq_verify_key(&state.http, &key).await?;
    let mut cached = state.groq_key.0.lock().await;
    let stored = key.clone();
    blocking(move || write_keychain(&stored)).await?;
    *cached = Some(Some(key));
    Ok(GroqKeyStatus { saved: true })
}

/// Deletes the saved key. Groq choices stay selected and fall back to the on-device models.
#[tauri::command]
pub(crate) async fn remove_groq_api_key(state: State<'_, AppState>) -> AppResult<GroqKeyStatus> {
    let mut cached = state.groq_key.0.lock().await;
    blocking(delete_keychain).await?;
    *cached = Some(None);
    Ok(GroqKeyStatus { saved: false })
}

/// Trimmed; `None` for a blank key or one with inner whitespace (a paste that caught more).
fn normalize_key(key: &str) -> Option<String> {
    let key = key.trim();
    (!key.is_empty() && !key.chars().any(char::is_whitespace)).then(|| key.to_owned())
}

/// Keychain calls block (and may show the system's access prompt), so they run off the async
/// workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| AppError::other(format!("Keychain access stopped: {error}")))?
}

fn read_keychain() -> AppResult<Option<String>> {
    match security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(bytes) => Ok(String::from_utf8(bytes)
            .ok()
            .and_then(|key| normalize_key(&key))),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(keychain_error("read", &error)),
    }
}

fn write_keychain(key: &str) -> AppResult<()> {
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        KEYCHAIN_ACCOUNT,
        key.as_bytes(),
    )
    .map_err(|error| keychain_error("save", &error))
}

fn delete_keychain() -> AppResult<()> {
    match security_framework::passwords::delete_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
    {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(keychain_error("remove", &error)),
    }
}

fn keychain_error(action: &str, error: &security_framework::base::Error) -> AppError {
    AppError::other(format!(
        "Could not {action} the Groq API key in the Keychain: {error}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_trimmed_and_blank_or_broken_pastes_refused() {
        assert_eq!(
            normalize_key("  gsk_abc123 \n").as_deref(),
            Some("gsk_abc123")
        );
        assert_eq!(normalize_key("   "), None);
        assert_eq!(normalize_key("gsk_abc 123"), None);
    }

    #[test]
    fn release_and_development_builds_use_separate_keychain_items() {
        assert!(KEYCHAIN_SERVICE.starts_with("dev.yuyudhan.simplevoice"));
        assert_eq!(
            KEYCHAIN_SERVICE.contains(".dev."),
            cfg!(debug_assertions),
            "{KEYCHAIN_SERVICE}"
        );
    }

    #[test]
    fn the_key_cache_never_prints_the_key() {
        let cache = KeyCache(tokio::sync::Mutex::new(Some(Some("gsk_secret".to_owned()))));
        assert!(!format!("{cache:?}").contains("gsk_secret"));
    }
}
