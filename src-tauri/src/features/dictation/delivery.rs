// FilePath: src-tauri/src/features/dictation/delivery.rs
//! Puts text into the frontmost app: clipboard, then Cmd+V through the engine helper, in
//! session order.

use std::sync::atomic::Ordering;
use std::time::Duration;

use sv_domain::{AppError, AppResult};
use tauri::{AppHandle, Manager};

use crate::features::permissions;
use crate::state::AppState;

/// Long enough for the target app to read the clipboard after Cmd+V.
const RESTORE_DELAY: Duration = Duration::from_millis(800);
const ACCESSIBILITY_MISSING: &str = "Text copied — grant Accessibility so Simple Voice can paste";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Delivery {
    Pasted,
    /// A newer session was already pasted; pasting this one now would put it out of order.
    Dropped,
    /// The text could not be pasted; the message tells the user what happened and what to do.
    NotPasted(String),
}

/// Pastes `text` as given unless a newer session's text already went out.
pub(crate) async fn deliver_in_order(
    app: &AppHandle,
    session: u64,
    text: &str,
    restore_clipboard: bool,
) -> Delivery {
    let state = app.state::<AppState>();
    if state
        .dictation
        .delivered
        .fetch_max(session, Ordering::SeqCst)
        > session
    {
        return Delivery::Dropped;
    }
    let text = text.to_owned();
    let previous = match replace_clipboard(&text) {
        Ok(previous) => previous,
        Err(error) => return Delivery::NotPasted(format!("Could not use the clipboard: {error}")),
    };
    match state.engine.paste().await {
        Ok(()) => {
            if restore_clipboard {
                if let Some(previous) = previous {
                    schedule_restore(text, previous);
                }
            }
            Delivery::Pasted
        }
        Err(AppError::Permission(_)) => {
            if let Err(error) = permissions::announce(app).await {
                tracing::debug!(%error, "permission refresh after paste failure failed");
            }
            Delivery::NotPasted(ACCESSIBILITY_MISSING.to_owned())
        }
        Err(error) => Delivery::NotPasted(format!("Text copied — paste failed: {error}")),
    }
}

/// Back-to-back dictations must not run together (`one.Two`), so pasted text ends in whitespace.
/// History keeps the text as transcribed; only the paste carries the separator. Edits replace a
/// selection and are pasted without one.
pub(crate) fn with_separator(text: &str) -> String {
    if text.is_empty() || text.ends_with(char::is_whitespace) {
        return text.to_owned();
    }
    format!("{text} ")
}

pub(crate) fn copy_to_clipboard(text: &str) -> AppResult<()> {
    let mut clipboard = arboard::Clipboard::new().map_err(AppError::other)?;
    clipboard.set_text(text).map_err(AppError::other)
}

/// Sets the clipboard and returns the text it held before, if any.
fn replace_clipboard(text: &str) -> AppResult<Option<String>> {
    let mut clipboard = arboard::Clipboard::new().map_err(AppError::other)?;
    let previous = clipboard.get_text().ok();
    clipboard.set_text(text).map_err(AppError::other)?;
    Ok(previous)
}

/// Puts the user's clipboard back, unless something else (the user, another dictation) has
/// replaced our text in the meantime.
fn schedule_restore(ours: String, previous: String) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(RESTORE_DELAY).await;
        let restored = arboard::Clipboard::new().and_then(|mut clipboard| {
            if clipboard.get_text().ok().as_deref() == Some(ours.as_str()) {
                clipboard.set_text(previous)?;
            }
            Ok(())
        });
        if let Err(error) = restored {
            tracing::warn!(%error, "could not restore the clipboard");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::with_separator;

    #[test]
    fn separator_is_added_only_when_text_does_not_already_end_in_whitespace() {
        assert_eq!(with_separator("First thought."), "First thought. ");
        assert_eq!(with_separator("ok"), "ok ");
        assert_eq!(with_separator("- one\n- two\n"), "- one\n- two\n");
        assert_eq!(with_separator("already "), "already ");
        assert_eq!(with_separator(""), "");
    }
}
