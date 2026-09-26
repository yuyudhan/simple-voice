// FilePath: src-tauri/src/lib.rs
//! Simple Voice core: Tauri wiring for the dictation coordinator, commands, windows, tray,
//! shortcuts and the Swift engine helper.
#![forbid(unsafe_code)]

mod events;
mod features;
mod platform;
mod state;

use std::time::Duration;

use sv_domain::{AppError, DictationPhase, DictationState, Settings};
use sv_storage::Db;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

use crate::features::dictation::{self, DictationHandle};
use crate::platform::{dock, engine_process, overlay, shortcuts, tray, windows};
use crate::state::AppState;

/// Builds and runs the app; returns when the user quits.
pub fn run() -> tauri::Result<()> {
    init_tracing();
    let app = tauri::Builder::default()
        // Must be registered first so a second launch focuses this one before anything else runs.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app)
        }))
        .plugin(shortcuts::plugin())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![dock::BACKGROUND_ARG]),
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            features::settings::get_settings,
            features::settings::update_settings,
            features::settings::set_groq_api_key,
            features::settings::verify_groq_api_key,
            features::settings::set_custom_api_key,
            features::settings::test_post_processing,
            features::settings::set_database_dir,
            features::settings::suspend_shortcuts,
            features::settings::list_microphones,
            features::settings::preview_sound,
            features::settings::app_info,
            features::history::list_history,
            features::history::delete_history,
            features::history::clear_history,
            features::history::retry_history,
            features::history::copy_text,
            features::dictionary::list_dictionary,
            features::dictionary::add_dictionary_entry,
            features::dictionary::update_dictionary_entry,
            features::dictionary::delete_dictionary_entry,
            features::dictionary::import_vocabulary,
            features::insights::get_insights,
            features::models::list_models,
            features::models::download_model,
            features::models::delete_model,
            features::permissions::get_permissions,
            features::permissions::request_permission,
            features::permissions::open_permission_settings,
            features::updates::get_update_status,
            features::updates::check_for_updates,
            features::updates::install::install_update,
            dictation::start_dictation,
            dictation::stop_dictation,
            dictation::cancel_dictation,
            dictation::toggle_dictation,
        ])
        .build(tauri::generate_context!())?;

    app.run(|app, event| match event {
        RunEvent::Reopen { .. } => windows::show_main(app),
        RunEvent::Exit => engine_process::shutdown(app),
        _ => {}
    });
    Ok(())
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    // A second init (tests, embedding) keeps the first subscriber, which is what we want.
    if let Err(error) = tracing_subscriber::fmt().with_env_filter(filter).try_init() {
        tracing::debug!(%error, "tracing already initialised");
    }
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let launched_in_background = std::env::args().any(|arg| arg == dock::BACKGROUND_ARG);

    let db = tauri::async_runtime::block_on(Db::open());
    let settings = match &db {
        Ok(db) => tauri::async_runtime::block_on(db.settings()).unwrap_or_else(|error| {
            tracing::error!(%error, "could not read settings; using defaults");
            Settings::default()
        }),
        Err(error) => {
            tracing::error!(%error, "could not open the database");
            Settings::default()
        }
    };
    let startup_error = db.as_ref().err().cloned();

    let (handle, control) = DictationHandle::new();
    let version = app.package_info().version.to_string();
    app.manage(AppState::new(db, handle, version));
    app.manage(shortcuts::ShortcutRegistry::default());
    app.manage(overlay::OverlayState::default());
    app.manage(engine_process::EngineProcess::default());
    app.manage(dock::DockSetting::default());
    let events_app = app.clone();
    app.state::<AppState>()
        .engine
        .set_event_handler(Box::new(move |event| {
            shortcuts::on_engine_event(&events_app, event);
        }));

    windows::create_main(app, !launched_in_background || startup_error.is_some())?;
    windows::install_app_menu(app)?;
    tray::create(app)?;

    dock::apply_dock(app, settings.show_in_dock);
    dock::sync_autostart(app, settings.launch_at_login);
    overlay::set_always(app, settings.show_bar_always);
    if let Err(error) = shortcuts::register(app, &settings.hold_shortcut, &settings.toggle_shortcut)
    {
        tracing::warn!(%error, "could not register the dictation shortcuts");
    }

    dictation::spawn_coordinator(app.clone(), control);
    engine_process::start(app.clone());
    features::permissions::spawn_poller(app.clone());
    features::updates::spawn_checker(app.clone());

    if let Some(error) = startup_error {
        report_startup_error(app.clone(), error);
    }
    Ok(())
}

/// The webview needs a moment to load and subscribe before it can show the error.
fn report_startup_error(app: AppHandle, error: AppError) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        let mut state = DictationState::new(DictationPhase::Error, 0);
        state.message = Some(format!("Simple Voice could not open its database. {error}"));
        events::emit(&app, events::DICTATION_STATE, state);
    });
}
