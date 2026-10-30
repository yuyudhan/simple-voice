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
use sv_engine::EngineEvent;
use sv_storage::Db;
use tauri::{AppHandle, Manager, RunEvent};

use crate::features::dictation::{self, DictationHandle};
use crate::platform::{dock, engine_process, login_item, overlay, shortcuts, tray, windows};
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
            features::settings::test_post_processing,
            features::settings::set_database_dir,
            features::settings::suspend_shortcuts,
            features::settings::list_microphones,
            features::settings::list_languages,
            features::settings::list_known_apps,
            features::settings::preview_sound,
            features::settings::app_info,
            features::groq::groq_key_status,
            features::groq::save_groq_api_key,
            features::groq::remove_groq_api_key,
            features::history::list_history,
            features::history::delete_history,
            features::history::clear_history,
            features::history::retry_history,
            features::history::copy_text,
            features::dictionary::list_dictionary,
            features::dictionary::list_vocabulary_packs,
            features::dictionary::add_dictionary_entry,
            features::dictionary::update_dictionary_entry,
            features::dictionary::delete_dictionary_entry,
            features::dictionary::import_vocabulary,
            features::dictionary::export_vocabulary,
            features::snippets::list_snippets,
            features::snippets::add_snippet,
            features::snippets::update_snippet,
            features::snippets::delete_snippet,
            features::meetings::list_meetings,
            features::meetings::get_meeting,
            features::meetings::meeting_readiness,
            features::meetings::start_meeting,
            features::meetings::stop_meeting,
            features::meetings::cancel_meeting,
            features::meetings::delete_meeting,
            features::meetings::rename_meeting,
            features::meetings::rename_speaker,
            features::meetings::update_segment,
            features::meetings::summarize_meeting,
            features::meetings::ask_meeting,
            features::meetings::translate_meeting,
            features::meetings::cancel_meeting_job,
            features::meetings::retry_meeting,
            features::meetings::export_meeting,
            features::insights::get_insights,
            features::insights::get_model_insights,
            features::models::list_models,
            features::models::download_model,
            features::models::delete_model,
            features::models::smart_select_plan,
            engine_process::engine_status,
            engine_process::restart_engine,
            features::permissions::get_permissions,
            features::permissions::request_permission,
            features::permissions::open_permission_settings,
            features::updates::get_update_status,
            features::updates::check_for_updates,
            features::updates::install::install_update,
            features::updates::restart::dismiss_update_notice,
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
    // A launch at login stays in the menu bar; the check only runs when the setting is on, so a
    // manual launch right after logging in still opens the window when it is off.
    let launched_at_login = settings.launch_at_login && login_item::launched_at_login();

    let (handle, control) = DictationHandle::new();
    let version = app.package_info().version.to_string();
    app.manage(AppState::new(db, handle, version));
    app.manage(shortcuts::ShortcutRegistry::default());
    app.manage(overlay::OverlayState::new(settings.pasted_text_seconds));
    app.manage(engine_process::EngineProcess::default());
    app.manage(dock::DockSetting::default());
    app.manage(features::meetings::MeetingsState::default());
    let events_app = app.clone();
    app.state::<AppState>()
        .engine
        .set_event_handler(Box::new(move |event| match event {
            EngineEvent::FnKey { action } => shortcuts::on_fn_key(&events_app, action),
            EngineEvent::StreamPartial {
                stream_id,
                text,
                stable,
            } => dictation::on_stream_partial(&events_app, stream_id, &text, &stable),
            EngineEvent::StreamFailed { stream_id, reason } => {
                dictation::on_stream_failed(&events_app, stream_id, &reason);
            }
            EngineEvent::SystemAudioFailed { reason } => {
                features::meetings::on_system_audio_failed(&events_app, reason);
            }
        }));

    windows::create_main(app, !launched_at_login || startup_error.is_some())?;
    windows::install_app_menu(app)?;
    tray::create(app)?;

    dock::apply_dock(app, settings.show_in_dock);
    overlay::set_always(app, settings.show_bar_always);
    let accelerators = shortcuts::Accelerators::of(&settings);
    if let Err(error) = shortcuts::register(app, accelerators) {
        tracing::warn!(%error, "could not register the shortcuts");
        // Dictation keeps working when only the edit shortcut is taken by another app.
        if !accelerators.edit.is_empty() {
            let without_edit = shortcuts::Accelerators {
                edit: "",
                ..accelerators
            };
            if let Err(error) = shortcuts::register(app, without_edit) {
                tracing::warn!(%error, "could not register the dictation shortcuts");
            }
        }
    }

    dictation::spawn_coordinator(app.clone(), control);
    engine_process::start(app.clone());
    features::permissions::spawn_poller(app.clone());
    features::meetings::spawn_recovery(app.clone());
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
