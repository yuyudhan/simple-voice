// FilePath: src-tauri/src/platform/shortcuts.rs
//! Global shortcuts: hold-to-talk, toggle, and Esc (cancel) while recording.

use std::str::FromStr;
use std::sync::Mutex;

use sv_domain::{AppError, AppResult};
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::features::dictation::Control;
use crate::state::{lock, AppState};

/// What a registered shortcut does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Binding {
    Hold,
    Toggle,
    /// Hold and toggle share one accelerator: a tap toggles, a long press is push-to-talk.
    Both,
    Escape,
}

/// Registration calls hop to the main thread and wait for it, while the shortcut handler runs
/// on the main thread. The handler therefore reads only `lookup`, which is never held across a
/// registration call; `inner` serializes registrations.
#[derive(Debug, Default)]
pub(crate) struct ShortcutRegistry {
    inner: Mutex<Registered>,
    lookup: Mutex<Lookup>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Lookup {
    hold: Option<Shortcut>,
    toggle: Option<Shortcut>,
}

#[derive(Debug, Default)]
struct Registered {
    hold: Option<Shortcut>,
    toggle: Option<Shortcut>,
    escape: bool,
    suspended: bool,
}

impl Registered {
    fn bindings(&self) -> Vec<Shortcut> {
        let mut shortcuts: Vec<Shortcut> = self.hold.iter().chain(&self.toggle).copied().collect();
        shortcuts.dedup();
        shortcuts
    }
}

pub(crate) fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(on_shortcut)
        .build()
}

fn escape() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

fn on_shortcut(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    let lookup = *lock(&app.state::<ShortcutRegistry>().lookup);
    let is_hold = lookup.hold.as_ref() == Some(shortcut);
    let is_toggle = lookup.toggle.as_ref() == Some(shortcut);
    let binding = match (is_hold, is_toggle) {
        (true, true) => Binding::Both,
        (true, false) => Binding::Hold,
        (false, true) => Binding::Toggle,
        (false, false) if *shortcut == escape() => Binding::Escape,
        (false, false) => return,
    };
    let pressed = event.state() == ShortcutState::Pressed;
    app.state::<AppState>()
        .dictation
        .send(Control::Shortcut(binding, pressed));
}

fn parse(accelerator: &str) -> AppResult<Shortcut> {
    Shortcut::from_str(accelerator.trim()).map_err(|_| unavailable(accelerator))
}

fn unavailable(accelerator: &str) -> AppError {
    AppError::invalid(format!(
        "{accelerator} is already used by another app or is invalid"
    ))
}

fn register_all(app: &AppHandle, shortcuts: &[Shortcut]) -> Result<(), usize> {
    let manager = app.global_shortcut();
    for (index, shortcut) in shortcuts.iter().enumerate() {
        if manager.is_registered(*shortcut) {
            continue;
        }
        if let Err(error) = manager.register(*shortcut) {
            tracing::warn!(%error, shortcut = ?shortcut, "shortcut registration failed");
            return Err(index);
        }
    }
    Ok(())
}

fn unregister_all(app: &AppHandle, shortcuts: &[Shortcut]) {
    let manager = app.global_shortcut();
    for shortcut in shortcuts {
        if manager.is_registered(*shortcut) {
            if let Err(error) = manager.unregister(*shortcut) {
                tracing::warn!(%error, shortcut = ?shortcut, "shortcut unregistration failed");
            }
        }
    }
}

/// Replaces the hold and toggle shortcuts. Both are parsed and registered before anything is
/// committed; on failure the previous shortcuts are registered again and nothing changes.
pub(crate) fn register(app: &AppHandle, hold: &str, toggle: &str) -> AppResult<()> {
    let hold_shortcut = parse(hold)?;
    let toggle_shortcut = parse(toggle)?;
    let registry = app.state::<ShortcutRegistry>();
    let mut registered = lock(&registry.inner);

    let previous = registered.bindings();
    unregister_all(app, &previous);

    let mut next = vec![hold_shortcut, toggle_shortcut];
    next.dedup();
    if let Err(failed) = register_all(app, &next) {
        let accelerator = if next.get(failed) == Some(&hold_shortcut) {
            hold
        } else {
            toggle
        };
        unregister_all(app, &next);
        if !registered.suspended {
            if let Err(index) = register_all(app, &previous) {
                tracing::warn!(index, "could not restore the previous shortcuts");
            }
        }
        return Err(unavailable(accelerator));
    }

    registered.hold = Some(hold_shortcut);
    registered.toggle = Some(toggle_shortcut);
    // Saving a new shortcut ends the capture that suspended them.
    registered.suspended = false;
    *lock(&registry.lookup) = Lookup {
        hold: Some(hold_shortcut),
        toggle: Some(toggle_shortcut),
    };
    Ok(())
}

/// While the user records a new shortcut in Settings the current ones must not fire.
pub(crate) fn suspend(app: &AppHandle, suspended: bool) -> AppResult<()> {
    let registry = app.state::<ShortcutRegistry>();
    let mut registered = lock(&registry.inner);
    registered.suspended = suspended;
    let bindings = registered.bindings();
    if suspended {
        unregister_all(app, &bindings);
        return Ok(());
    }
    register_all(app, &bindings).map_err(|_| {
        AppError::invalid(
            "A dictation shortcut is already used by another app; choose another in Settings",
        )
    })
}

/// Esc cancels only while recording, so it is registered for exactly that span and every other
/// app keeps its Escape key the rest of the time.
pub(crate) fn set_escape(app: &AppHandle, active: bool) {
    let registry = app.state::<ShortcutRegistry>();
    let mut registered = lock(&registry.inner);
    if registered.escape == active {
        return;
    }
    registered.escape = active;
    if active {
        if register_all(app, &[escape()]).is_err() {
            tracing::warn!("Esc could not be registered; cancel from the tray instead");
        }
    } else {
        unregister_all(app, &[escape()]);
    }
}
