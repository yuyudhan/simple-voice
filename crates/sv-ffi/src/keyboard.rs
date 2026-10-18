// FilePath: crates/sv-ffi/src/keyboard.rs
//! JNI exports for `dev.yuyudhan.simplevoice.mobile.keyboard.SvCore`
//! (mobile/src-tauri/gen/android/.../keyboard/SvCore.kt). Names follow JNI's
//! `Java_<package>_<class>_<method>` rule; renaming the Kotlin class means renaming these.
//! Calls that reach the network block their thread, so Kotlin makes them off the main thread.

use std::path::{Path, PathBuf};

use jni::objects::{JClass, JString};
use jni::sys::{jboolean, jlong, jstring, JNI_FALSE};
use jni::JNIEnv;

use sv_mobile::Recording;

use crate::core;

/// `SvCore.init(dataDir: String): String` - empty on success, otherwise the error message.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_init<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    data_dir: JString<'local>,
) -> jstring {
    let message = match read(&mut env, &data_dir) {
        Some(dir) => match core::init(Path::new(&dir)) {
            Ok(()) => String::new(),
            Err(error) => error.to_string(),
        },
        None => "Could not read the data directory".to_owned(),
    };
    to_java(&mut env, &message)
}

/// `SvCore.status(online: Boolean): String` - `{line, ready, needsKey, maxSeconds, editMaxChars}`.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_status<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    online: jboolean,
) -> jstring {
    to_java(&mut env, &core::status(online != JNI_FALSE))
}

/// `SvCore.dictate(wavPath: String, durationMs: Long, appName: String?, appId: String?): String`
/// - `{kind: "text", id, text, unformatted}`, `{kind: "noSpeech"}` or `{kind: "failed", id?, message}`.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_dictate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    wav_path: JString<'local>,
    duration_ms: jlong,
    app_name: JString<'local>,
    app_id: JString<'local>,
) -> jstring {
    let Some(path) = read(&mut env, &wav_path) else {
        return to_java(
            &mut env,
            r#"{"kind":"failed","message":"Could not read the recording path"}"#,
        );
    };
    let app_name = read(&mut env, &app_name);
    let app_id = read(&mut env, &app_id);
    let answer = core::dictate(PathBuf::from(path), duration_ms, app_name, app_id);
    to_java(&mut env, &answer)
}

/// `SvCore.retry(id: Long): String` - the same shapes as `dictate`.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_retry<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jlong,
) -> jstring {
    to_java(&mut env, &core::retry(id))
}

/// `SvCore.edit(selection: String, wavPath: String, durationMs: Long, appName: String?,
/// appId: String?, retryOf: Long): String` - `{kind: "text", id, text}`, `{kind: "noSpeech"}` or
/// `{kind: "failed", id?, message}`. `retryOf` is 0 for a new edit.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_edit<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    selection: JString<'local>,
    wav_path: JString<'local>,
    duration_ms: jlong,
    app_name: JString<'local>,
    app_id: JString<'local>,
    retry_of: jlong,
) -> jstring {
    let (Some(selection), Some(path)) = (read(&mut env, &selection), read(&mut env, &wav_path))
    else {
        return to_java(
            &mut env,
            r#"{"kind":"failed","message":"Could not read the selection or the recording"}"#,
        );
    };
    let recording = Recording {
        wav_path: PathBuf::from(path),
        duration_ms,
        app_name: read(&mut env, &app_name),
        app_id: read(&mut env, &app_id),
    };
    let answer = core::edit(selection, recording, (retry_of > 0).then_some(retry_of));
    to_java(&mut env, &answer)
}

/// `SvCore.markNotInserted(id: Long, reason: String)` - the text never reached its field.
#[no_mangle]
pub extern "system" fn Java_dev_yuyudhan_simplevoice_mobile_keyboard_SvCore_markNotInserted<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: jlong,
    reason: JString<'local>,
) {
    let reason = read(&mut env, &reason).unwrap_or_default();
    core::mark_not_inserted(id, &reason);
}

/// A Kotlin string, or `None` for a null reference or one the JVM cannot read.
fn read(env: &mut JNIEnv<'_>, value: &JString<'_>) -> Option<String> {
    if value.is_null() {
        return None;
    }
    env.get_string(value).ok().map(String::from)
}

/// A Java string, or null when the JVM cannot allocate one (it then has an exception pending,
/// which Kotlin raises as soon as the call returns).
fn to_java(env: &mut JNIEnv<'_>, text: &str) -> jstring {
    match env.new_string(text) {
        Ok(string) => string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
