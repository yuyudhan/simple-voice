// FilePath: src-tauri/src/platform/shortcuts.rs
//! Global shortcuts: hold-to-talk, toggle, hold-to-edit (edit mode), and Esc (cancel) while
//! recording.
//!
//! Key combinations go through the global-shortcut plugin (Carbon hot keys). Carbon cannot
//! register a lone modifier, so the Fn key is watched by the engine helper, which reports it as
//! `fn_key` events (see `on_engine_event`).

use std::str::FromStr;
use std::sync::Mutex;

use sv_domain::settings::FN_KEY_ACCELERATOR;
use sv_domain::{AppError, AppResult, Settings};
use sv_engine::{EngineEvent, FnKeyAction};
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
    /// Hold to record an instruction that edits the selected text.
    Edit,
    Escape,
}

/// The configured accelerators; `edit` is empty while edit mode is off.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Accelerators<'a> {
    pub(crate) hold: &'a str,
    pub(crate) toggle: &'a str,
    pub(crate) edit: &'a str,
}

impl<'a> Accelerators<'a> {
    pub(crate) fn of(settings: &'a Settings) -> Self {
        Self {
            hold: &settings.hold_shortcut,
            toggle: &settings.toggle_shortcut,
            edit: &settings.edit_shortcut,
        }
    }
}

/// Registration calls hop to the main thread and wait for it, while the shortcut handler runs
/// on the main thread. The handler therefore reads only `lookup`, which is never held across a
/// registration call; `inner` serializes registrations.
#[derive(Debug, Default)]
pub(crate) struct ShortcutRegistry {
    inner: Mutex<Registered>,
    lookup: Mutex<Lookup>,
}

/// A configured dictation shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    Key(Shortcut),
    Fn,
}

impl Trigger {
    fn key(self) -> Option<Shortcut> {
        match self {
            Trigger::Key(shortcut) => Some(shortcut),
            Trigger::Fn => None,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Lookup {
    hold: Option<Trigger>,
    toggle: Option<Trigger>,
    edit: Option<Shortcut>,
    /// Fn events keep arriving while the user records a new shortcut; they are dropped here.
    suspended: bool,
}

impl Lookup {
    fn watches_fn(&self) -> bool {
        self.hold == Some(Trigger::Fn) || self.toggle == Some(Trigger::Fn)
    }
}

#[derive(Debug, Default)]
struct Registered {
    hold: Option<Trigger>,
    toggle: Option<Trigger>,
    edit: Option<Shortcut>,
    escape: bool,
    suspended: bool,
}

impl Registered {
    /// The key combinations registered with the plugin.
    fn bindings(&self) -> Vec<Shortcut> {
        let mut shortcuts: Vec<Shortcut> = self
            .hold
            .iter()
            .chain(&self.toggle)
            .filter_map(|trigger| trigger.key())
            .chain(self.edit)
            .collect();
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
    let is_hold = lookup.hold == Some(Trigger::Key(*shortcut));
    let is_toggle = lookup.toggle == Some(Trigger::Key(*shortcut));
    let binding = match (is_hold, is_toggle) {
        (true, true) => Binding::Both,
        (true, false) => Binding::Hold,
        (false, true) => Binding::Toggle,
        (false, false) if lookup.edit == Some(*shortcut) => Binding::Edit,
        (false, false) if *shortcut == escape() => Binding::Escape,
        (false, false) => return,
    };
    let pressed = event.state() == ShortcutState::Pressed;
    app.state::<AppState>()
        .dictation
        .send(Control::Shortcut(binding, pressed));
}

fn parse(accelerator: &str) -> AppResult<Trigger> {
    let accelerator = accelerator.trim();
    if accelerator.eq_ignore_ascii_case(FN_KEY_ACCELERATOR) {
        return Ok(Trigger::Fn);
    }
    Shortcut::from_str(accelerator)
        .map(Trigger::Key)
        .map_err(|_| unavailable(accelerator))
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

/// Edit mode needs a key combination of its own: Fn belongs to dictation's watch, and a
/// combination shared with a dictation shortcut could not tell the two apart.
fn parse_edit(accelerator: &str, dictation: [Trigger; 2]) -> AppResult<Option<Shortcut>> {
    if accelerator.trim().is_empty() {
        return Ok(None);
    }
    match parse(accelerator)? {
        Trigger::Fn => Err(AppError::invalid(
            "Fn can't be the edit shortcut; choose a key combination",
        )),
        trigger if dictation.contains(&trigger) => Err(AppError::invalid(format!(
            "{} is already a dictation shortcut; choose another for editing",
            accelerator.trim()
        ))),
        Trigger::Key(shortcut) => Ok(Some(shortcut)),
    }
}

/// Replaces the hold, toggle and edit shortcuts. All are parsed and registered before anything
/// is committed; on failure the previous shortcuts are registered again and nothing changes.
pub(crate) fn register(app: &AppHandle, accelerators: Accelerators<'_>) -> AppResult<()> {
    let hold_trigger = parse(accelerators.hold)?;
    let toggle_trigger = parse(accelerators.toggle)?;
    let edit_shortcut = parse_edit(accelerators.edit, [hold_trigger, toggle_trigger])?;
    let registry = app.state::<ShortcutRegistry>();
    let mut registered = lock(&registry.inner);

    let previous = registered.bindings();
    unregister_all(app, &previous);

    let mut next: Vec<Shortcut> = [hold_trigger, toggle_trigger]
        .into_iter()
        .filter_map(Trigger::key)
        .chain(edit_shortcut)
        .collect();
    next.dedup();
    if let Err(failed) = register_all(app, &next) {
        let failed = next.get(failed).copied();
        let accelerator = if failed.is_some() && failed == edit_shortcut {
            accelerators.edit
        } else if failed.map(Trigger::Key) == Some(hold_trigger) {
            accelerators.hold
        } else {
            accelerators.toggle
        };
        unregister_all(app, &next);
        if !registered.suspended {
            if let Err(index) = register_all(app, &previous) {
                tracing::warn!(index, "could not restore the previous shortcuts");
            }
        }
        return Err(unavailable(accelerator));
    }

    registered.hold = Some(hold_trigger);
    registered.toggle = Some(toggle_trigger);
    registered.edit = edit_shortcut;
    // Saving a new shortcut ends the capture that suspended them.
    registered.suspended = false;
    *lock(&registry.lookup) = Lookup {
        hold: Some(hold_trigger),
        toggle: Some(toggle_trigger),
        edit: edit_shortcut,
        suspended: false,
    };
    drop(registered);
    sync_fn_key(app);
    Ok(())
}

/// Tells the engine helper whether to watch the Fn key. Called after every registration and
/// whenever the helper (re)starts; the desired state is read when the request is sent, so
/// overlapping calls converge on the latest registration.
pub(crate) fn sync_fn_key(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let watch = lock(&app.state::<ShortcutRegistry>().lookup).watches_fn();
        match app.state::<AppState>().engine.watch_fn_key(watch).await {
            Ok(true) if watch => tracing::info!("watching the Fn key"),
            Ok(false) if watch => {
                tracing::warn!("the Fn key needs Accessibility access; waiting for it");
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(%error, watch, "could not update the Fn key watch"),
        }
    });
}

/// Turns the helper's Fn key events into the same controls a key combination produces. A toggle
/// on Fn fires on release, so Fn used as a modifier (Fn+Arrow) never toggles; the helper reports
/// that case as `Chord`, which discards a recording the press already started.
pub(crate) fn on_engine_event(app: &AppHandle, event: EngineEvent) {
    let EngineEvent::FnKey { action } = event;
    let lookup = *lock(&app.state::<ShortcutRegistry>().lookup);
    if lookup.suspended {
        return;
    }
    let control = match (
        lookup.hold == Some(Trigger::Fn),
        lookup.toggle == Some(Trigger::Fn),
    ) {
        (false, false) => return,
        (false, true) => match action {
            FnKeyAction::Tap | FnKeyAction::Release => Control::Shortcut(Binding::Toggle, true),
            FnKeyAction::Press | FnKeyAction::Chord => return,
        },
        (true, shared) => {
            let binding = if shared { Binding::Both } else { Binding::Hold };
            match action {
                FnKeyAction::Press => Control::Shortcut(binding, true),
                FnKeyAction::Release => Control::Shortcut(binding, false),
                FnKeyAction::Chord => Control::ShortcutChord(binding),
                // A tap is too short to be a hold, so on a shared key it can only mean toggle.
                FnKeyAction::Tap if shared => Control::Shortcut(Binding::Toggle, true),
                FnKeyAction::Tap => return,
            }
        }
    };
    app.state::<AppState>().dictation.send(control);
}

/// While the user records a new shortcut in Settings the current ones must not fire.
pub(crate) fn suspend(app: &AppHandle, suspended: bool) -> AppResult<()> {
    let registry = app.state::<ShortcutRegistry>();
    let mut registered = lock(&registry.inner);
    registered.suspended = suspended;
    lock(&registry.lookup).suspended = suspended;
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
