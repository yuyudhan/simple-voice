// FilePath: mobile/src-tauri/src/lib.rs
//! Simple Voice for iPhone and Android: the app screens (history, dictionary, insights,
//! settings) over the shared crates. Dictation happens in the keyboards; the app retries
//! failed dictations and holds the Groq key.
#![forbid(unsafe_code)]

mod events;
mod features;
mod state;

use sv_storage::Db;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

// Nothing here calls sv-ffi; naming it makes the linker keep the keyboard's JNI exports.
#[cfg(target_os = "android")]
use sv_ffi as _;

/// Builds and runs the app; returns when the system ends it.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            setup(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            features::settings::get_settings,
            features::settings::update_settings,
            features::settings::set_groq_api_key,
            features::settings::verify_groq_api_key,
            features::history::list_history,
            features::history::delete_history,
            features::history::retry_history,
            features::history::copy_text,
            features::dictionary::list_dictionary,
            features::dictionary::add_dictionary_entry,
            features::dictionary::update_dictionary_entry,
            features::dictionary::delete_dictionary_entry,
            features::dictionary::import_vocabulary,
            features::insights::get_insights,
            features::insights::get_model_insights,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        tracing::error!(%error, "the app stopped with an error");
    }
}

fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if let Err(error) = tracing_subscriber::fmt().with_env_filter(filter).try_init() {
        tracing::debug!(%error, "tracing already initialised");
    }
}

/// Opens the database in the app's private data directory. A database that fails to open
/// keeps its error in the state, so every screen reports it instead of the app not starting.
fn setup(app: &AppHandle) {
    let db = match app.path().app_data_dir() {
        Ok(dir) => tauri::async_runtime::block_on(Db::open_at(&dir)),
        Err(error) => Err(sv_domain::AppError::io(error)),
    };
    if let Err(error) = &db {
        tracing::error!(%error, "could not open the database");
    }
    app.manage(AppState::new(db, sv_mobile::http_client()));
}
