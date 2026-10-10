// FilePath: mobile/src-tauri/src/features/session/mod.rs
//! iPhone dictation (MB-7). The keyboard cannot record, so the app's Swift session records for
//! it and asks Rust, through the sv-native bridge, for the status line and for each dictation.
//! On Android the bridge carries nothing and this module stays idle.

use serde::Serialize;
use serde_json::Value;
use sv_domain::AppResult;
use sv_mobile::{Delivery, Dictation, KeyboardStatus, LocalTranscript, Reach, Recording};
use tauri::{AppHandle, Manager};
use tauri_plugin_sv_native::{NativeBridge, NativeRequest};

use crate::events;
use crate::state::AppState;

/// Not inserted because no keyboard confirmed it; the app copied it instead (D-15).
const COPIED_INSTEAD: &str = "The text field closed first; the text was copied";

/// Rust's answer to Swift's `status`: the keyboard line plus the session length.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeStatus {
    #[serde(flatten)]
    status: KeyboardStatus,
    session_minutes: u32,
}

/// Hands Swift the channel its requests arrive on.
pub(crate) fn listen(app: &AppHandle) {
    let app = app.clone();
    // `listen` waits for Swift to take the channel; never wait on the thread that starts the app.
    std::thread::spawn(move || {
        let handler = app.clone();
        app.state::<NativeBridge<tauri::Wry>>()
            .listen(move |id, request| {
                let app = handler.clone();
                tauri::async_runtime::spawn(async move {
                    let body = answer(&app, request).await;
                    send(app, id, body);
                });
            });
    });
}

/// Tells Swift to fetch a fresh status line, after a settings or Groq key change.
pub(crate) fn refresh(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<NativeBridge<tauri::Wry>>().refresh();
    });
}

fn send(app: AppHandle, id: u64, body: Value) {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<NativeBridge<tauri::Wry>>().reply(id, &body);
    });
}

async fn answer(app: &AppHandle, request: NativeRequest) -> Value {
    let state = app.state::<AppState>();
    let result = match request {
        NativeRequest::Status {
            online,
            on_device_speech,
        } => status(&state, online, on_device_speech).await,
        NativeRequest::Dictate {
            wav_path,
            duration_ms,
        } => dictate(app, &state, recording(wav_path, duration_ms)).await,
        NativeRequest::Local {
            wav_path,
            duration_ms,
            text,
            language,
            transcribe_ms,
        } => {
            let heard = LocalTranscript {
                text,
                language,
                transcribe_ms,
            };
            match state.db() {
                Ok(db) => {
                    let dictation =
                        sv_mobile::dictate_local(db, heard, recording(wav_path, duration_ms)).await;
                    events::emit(app, events::HISTORY_CHANGED, ());
                    to_json(&dictation)
                }
                Err(error) => Err(error),
            }
        }
        NativeRequest::Retry { history_id } => retry(app, &state, history_id).await,
        NativeRequest::NotInserted { history_id } => not_inserted(app, &state, history_id).await,
    };
    result.unwrap_or_else(|error| {
        to_json(&Dictation::Failed {
            id: None,
            message: error.to_string(),
        })
        .unwrap_or(Value::Null)
    })
}

async fn status(state: &AppState, online: bool, on_device_speech: bool) -> AppResult<Value> {
    let db = state.db()?;
    let reach = Reach {
        online,
        on_device_speech,
    };
    let status = sv_mobile::keyboard_status(db, reach).await;
    let session_minutes = db.settings().await?.session_minutes;
    to_json(&NativeStatus {
        status,
        session_minutes,
    })
}

async fn dictate(app: &AppHandle, state: &AppState, recording: Recording) -> AppResult<Value> {
    let dictation = sv_mobile::dictate(state.db()?, state.http()?, recording).await;
    events::emit(app, events::HISTORY_CHANGED, ());
    to_json(&dictation)
}

async fn retry(app: &AppHandle, state: &AppState, id: i64) -> AppResult<Value> {
    let outcome = sv_mobile::retry(state.db()?, state.http()?, id, Delivery::Inserted).await;
    events::emit(app, events::HISTORY_CHANGED, ());
    let dictation = match outcome {
        Ok((entry, text)) => Dictation::Text {
            id: entry.id,
            text,
            unformatted: false,
        },
        Err(error) => Dictation::Failed {
            id: Some(id),
            message: error.to_string(),
        },
    };
    to_json(&dictation)
}

async fn not_inserted(app: &AppHandle, state: &AppState, id: i64) -> AppResult<Value> {
    state
        .db()?
        .mark_history_not_pasted(id, COPIED_INSTEAD)
        .await?;
    events::emit(app, events::HISTORY_CHANGED, ());
    Ok(Value::Null)
}

/// The keyboard cannot tell which app it is typing in on iPhone, so the app is unknown.
fn recording(wav_path: String, duration_ms: i64) -> Recording {
    Recording {
        wav_path: wav_path.into(),
        duration_ms,
        app_name: None,
        app_id: None,
    }
}

fn to_json(value: &impl Serialize) -> AppResult<Value> {
    serde_json::to_value(value).map_err(sv_domain::AppError::other)
}
