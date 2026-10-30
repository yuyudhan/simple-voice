// FilePath: src-tauri/src/features/dictation/context.rs
//! Optional context from the focused text field. When the user turned it on, the recording
//! start asks the helper for a bounded stretch of the focused field around the caret (never
//! from excluded apps or secure fields), and the pipeline turns it into spelling hints for the
//! formatting pass and Whisper's prompt: names and terms near the caret and, in code editors
//! with IDE context on, the visible file names and identifiers. The field text itself is never
//! stored, logged or sent anywhere; only the hints live for the one dictation.

use std::time::Duration;

use sv_domain::{categorize, AppCategory, Settings};
use sv_engine::{FocusContext, FrontmostApp};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// Longest the context read may take; reading never delays the recording itself.
const READ_TIMEOUT: Duration = Duration::from_millis(600);
/// Longest the pipeline waits for a read still running when the recording stopped.
const WAIT_TIMEOUT: Duration = Duration::from_millis(800);

/// The spelling hints read at recording start.
pub(crate) type ContextTask = JoinHandle<Vec<String>>;

/// Which kinds of context this recording may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wanted {
    field: bool,
    ide: bool,
}

fn wanted(settings: &Settings, frontmost: Option<&FrontmostApp>) -> Wanted {
    let bundle_id = frontmost.and_then(|front| front.bundle_id.as_deref());
    let app_name = frontmost.and_then(|front| front.name.as_deref());
    let excluded = bundle_id.is_some_and(|id| {
        settings
            .context_excluded_apps
            .iter()
            .any(|excluded| excluded.eq_ignore_ascii_case(id.trim()))
    });
    Wanted {
        field: settings.context_allowed(bundle_id),
        // Identifiers from the editor stay on the Mac too.
        ide: settings.ide_context
            && !settings.dictation_uses_groq()
            && !excluded
            && categorize(bundle_id, app_name) == AppCategory::Code,
    }
}

/// Starts reading the focused field when the settings ask for context; `None` otherwise.
pub(crate) fn capture(
    app: &AppHandle,
    settings: &Settings,
    frontmost: Option<&FrontmostApp>,
) -> Option<ContextTask> {
    let wanted = wanted(settings, frontmost);
    if !wanted.field && !wanted.ide {
        return None;
    }
    let app = app.clone();
    let excluded = settings.context_excluded_apps.clone();
    let expected = frontmost.and_then(|front| front.bundle_id.clone());
    Some(tauri::async_runtime::spawn(async move {
        let engine = &app.state::<AppState>().engine;
        let read = tokio::time::timeout(
            READ_TIMEOUT,
            engine.focus_context(sv_text::context::FOCUS_MAX_CHARS, &excluded),
        )
        .await;
        match read {
            Ok(Ok(context)) => hints(&context, expected.as_deref(), wanted),
            Ok(Err(error)) => {
                tracing::info!(%error, "dictation context unavailable");
                Vec::new()
            }
            Err(_) => {
                tracing::info!("dictation context read timed out");
                Vec::new()
            }
        }
    }))
}

/// The hints of a finished read. A field the helper would not or could not read gives none,
/// and so does a read that landed in another app than the one the recording started in.
fn hints(context: &FocusContext, expected: Option<&str>, wanted: Wanted) -> Vec<String> {
    if let Some(reason) = &context.reason {
        // Only why, never what: the bundle id and the reason.
        tracing::info!(?reason, bundle = ?context.bundle_id, "dictation context not read");
        return Vec::new();
    }
    let same_app = match (expected, context.bundle_id.as_deref()) {
        (Some(expected), Some(read)) => expected.eq_ignore_ascii_case(read),
        _ => false,
    };
    if !same_app {
        tracing::info!("focus changed before the context was read; not used");
        return Vec::new();
    }
    let text = context.text.as_deref().unwrap_or_default();
    let mut terms = Vec::new();
    if wanted.field {
        terms.extend(sv_text::context::context_terms(text, context.caret));
    }
    if wanted.ide {
        for term in sv_text::context::ide_terms(text, context.window_title.as_deref()) {
            if !terms.contains(&term) {
                terms.push(term);
            }
        }
    }
    terms
}

/// The hints read at recording start, or none when the read is missing or too slow.
pub(crate) async fn collect(task: Option<ContextTask>) -> Vec<String> {
    let Some(task) = task else {
        return Vec::new();
    };
    match tokio::time::timeout(WAIT_TIMEOUT, task).await {
        Ok(Ok(terms)) => terms,
        Ok(Err(error)) => {
            tracing::warn!(%error, "dictation context read did not finish");
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use sv_engine::FocusUnavailable;

    use super::*;

    fn front(bundle_id: &str, name: &str) -> FrontmostApp {
        FrontmostApp {
            name: Some(name.to_owned()),
            bundle_id: Some(bundle_id.to_owned()),
        }
    }

    fn read(bundle_id: &str, text: &str) -> FocusContext {
        FocusContext {
            app_name: None,
            bundle_id: Some(bundle_id.to_owned()),
            window_title: Some("payments_service.rs — billing".to_owned()),
            role: Some("AXTextArea".to_owned()),
            text: Some(text.to_owned()),
            caret: None,
            reason: None,
        }
    }

    #[test]
    fn nothing_is_read_unless_asked_and_never_from_excluded_apps() {
        let mut settings = Settings::default();
        let notes = front("com.apple.Notes", "Notes");
        let code = front("com.microsoft.VSCode", "Code");
        let vault = front("com.1password.1password", "1Password");
        let none = Wanted {
            field: false,
            ide: false,
        };
        assert_eq!(wanted(&settings, Some(&notes)), none);
        assert_eq!(wanted(&settings, Some(&code)), none);

        settings.ide_context = true;
        assert_eq!(
            wanted(&settings, Some(&code)),
            Wanted {
                field: false,
                ide: true
            }
        );
        // IDE context reads code editors only.
        assert_eq!(wanted(&settings, Some(&notes)), none);

        settings.foreground_context = true;
        assert!(wanted(&settings, Some(&notes)).field);
        assert_eq!(wanted(&settings, Some(&vault)), none);
        settings.context_excluded_apps = vec!["com.microsoft.vscode".to_owned()];
        assert_eq!(wanted(&settings, Some(&code)), none);
    }

    #[test]
    fn unreadable_fields_and_changed_focus_give_no_hints() {
        let both = Wanted {
            field: true,
            ide: true,
        };
        let secure = FocusContext {
            reason: Some(FocusUnavailable::SecureField),
            text: None,
            ..read("com.apple.Safari", "")
        };
        assert!(hints(&secure, Some("com.apple.Safari"), both).is_empty());
        let denied = FocusContext {
            reason: Some(FocusUnavailable::PermissionDenied),
            ..read("com.apple.Notes", "Priya Raman")
        };
        assert!(hints(&denied, Some("com.apple.Notes"), both).is_empty());
        let moved = read("com.apple.Notes", "Priya Raman joined the call");
        assert!(hints(&moved, Some("com.apple.mail"), both).is_empty());
        assert!(hints(&moved, None, both).is_empty());
    }

    #[test]
    fn ide_hints_need_ide_context() {
        let editor = read(
            "com.microsoft.VSCode",
            "fn refund_invoice(invoice: &Invoice) -> PaymentResult {",
        );
        let ide_only = Wanted {
            field: false,
            ide: true,
        };
        let terms = hints(&editor, Some("com.microsoft.VSCode"), ide_only);
        assert!(
            terms.iter().any(|term| term == "payments_service.rs"),
            "{terms:?}"
        );
        let field_only = Wanted {
            field: true,
            ide: false,
        };
        let terms = hints(&editor, Some("com.microsoft.VSCode"), field_only);
        assert!(
            !terms.iter().any(|term| term == "payments_service.rs"),
            "{terms:?}"
        );
    }
}
