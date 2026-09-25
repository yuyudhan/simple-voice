// FilePath: src-tauri/src/platform/overlay.rs
//! The floating pill shown while dictating. It must never take focus: the dictated text is
//! pasted into whatever app was frontmost, so the overlay is non-focusable and click-through.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use sv_domain::DictationPhase;
use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

pub(crate) const OVERLAY: &str = "overlay";
const WIDTH: f64 = 200.0;
const HEIGHT: f64 = 56.0;
/// Logical points between the pill and the bottom of the work area (above the Dock).
const BOTTOM_MARGIN: f64 = 80.0;
const HIDE_DELAY: Duration = Duration::from_millis(1200);

#[derive(Debug, Default)]
pub(crate) struct OverlayState {
    always: AtomicBool,
    /// Bumped on every phase change so a pending hide is skipped once a newer phase arrives.
    generation: AtomicU64,
}

pub(crate) fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::default())
        .title("Simple Voice")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    window.set_ignore_cursor_events(true)?;
    Ok(())
}

/// Keeps the pill visible at all times when the user asked for it.
pub(crate) fn set_always(app: &AppHandle, always: bool) {
    app.state::<OverlayState>()
        .always
        .store(always, Ordering::SeqCst);
    if always {
        show(app);
    } else if !app
        .state::<crate::state::AppState>()
        .dictation
        .is_recording()
    {
        hide(app);
    }
}

/// Follows the published dictation phase: visible while a session is active, hidden shortly
/// after it ends.
pub(crate) fn follow(app: &AppHandle, phase: DictationPhase) {
    let overlay = app.state::<OverlayState>();
    let generation = overlay.generation.fetch_add(1, Ordering::SeqCst) + 1;
    match phase {
        DictationPhase::Idle => {
            if !overlay.always.load(Ordering::SeqCst) {
                hide(app);
            }
        }
        DictationPhase::Recording | DictationPhase::Transcribing | DictationPhase::Formatting => {
            show(app);
        }
        DictationPhase::Done | DictationPhase::Error | DictationPhase::Cancelled => {
            show(app);
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(HIDE_DELAY).await;
                let overlay = app.state::<OverlayState>();
                let unchanged = overlay.generation.load(Ordering::SeqCst) == generation;
                if unchanged && !overlay.always.load(Ordering::SeqCst) {
                    hide(&app);
                }
            });
        }
    }
}

fn window(app: &AppHandle) -> Option<WebviewWindow> {
    let window = app.get_webview_window(OVERLAY);
    if window.is_none() {
        tracing::warn!("overlay window missing");
    }
    window
}

fn show(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    if let Err(error) = position_under_cursor(app, &window) {
        tracing::debug!(%error, "could not position the overlay");
    }
    if let Err(error) = window.show() {
        tracing::warn!(%error, "could not show the overlay");
    }
}

fn hide(app: &AppHandle) {
    let Some(window) = window(app) else { return };
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "could not hide the overlay");
    }
}

/// Bottom-centre of the work area of the monitor the cursor is on, so the pill appears on the
/// screen the user is looking at.
fn position_under_cursor(app: &AppHandle, window: &WebviewWindow) -> tauri::Result<()> {
    let cursor = app.cursor_position()?;
    let monitor = match app.monitor_from_point(cursor.x, cursor.y)? {
        Some(monitor) => monitor,
        None => match app.primary_monitor()? {
            Some(monitor) => monitor,
            None => return Ok(()),
        },
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let width = WIDTH * scale;
    let height = HEIGHT * scale;
    let x = f64::from(area.position.x) + (f64::from(area.size.width) - width) / 2.0;
    let y =
        f64::from(area.position.y) + f64::from(area.size.height) - height - BOTTOM_MARGIN * scale;
    window.set_position(PhysicalPosition::new(x.round(), y.round()))
}
