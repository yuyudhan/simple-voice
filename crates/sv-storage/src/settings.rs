// FilePath: crates/sv-storage/src/settings.rs
//! Settings persistence: one row per field, keyed by the camelCase field name, value as JSON.
//! Missing rows take `Settings::default()`, so adding a setting never needs a migration. API keys
//! live in the same table under their own keys and never enter `Settings`.

use std::path::Path;

use serde_json::{Map, Value};
use sqlx::SqlitePool;
use sv_domain::{AppError, AppResult, Settings, SettingsPatch};

use crate::db::Db;

const GROQ_API_KEY: &str = "groqApiKey";
const CUSTOM_API_KEY: &str = "customApiKey";
const GROQ_API_KEY_ENV: &str = "GROQ_API_KEY";
/// Computed on every read; a stored row with one of these names is ignored.
const DERIVED_FIELDS: [&str; 3] = ["groqApiKeyPresent", "customApiKeyPresent", "databaseDir"];
const MIN_RECORDING_SECONDS: u32 = 10;
const MAX_RECORDING_SECONDS: u32 = 1800;

impl Db {
    pub async fn settings(&self) -> AppResult<Settings> {
        let inner = self.read().await;
        load_settings(&inner.pool, inner.database_dir()).await
    }

    /// Applies `patch`, validates the result, and saves every patched field in one transaction.
    /// Invalid input saves nothing.
    pub async fn update_settings(&self, patch: SettingsPatch) -> AppResult<Settings> {
        let inner = self.read().await;
        let mut next = load_settings(&inner.pool, inner.database_dir()).await?;
        let keys = patched_keys(&patch)?;
        apply_patch(&mut next, patch);
        validate(&next)?;

        let values = to_object(&next)?;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        for key in &keys {
            let Some(value) = values.get(key) else {
                continue;
            };
            let text = value.to_string();
            sqlx::query!(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                key.as_str(),
                text
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(next)
    }

    /// The stored Groq key, else `GROQ_API_KEY` from the environment.
    pub async fn groq_api_key(&self) -> AppResult<Option<String>> {
        let inner = self.read().await;
        Ok(stored_key(&inner.pool, GROQ_API_KEY)
            .await?
            .or_else(env_groq_key))
    }

    /// `None` or a blank key removes the stored key.
    pub async fn set_groq_api_key(&self, key: Option<String>) -> AppResult<()> {
        let inner = self.read().await;
        store_key(&inner.pool, GROQ_API_KEY, key).await
    }

    pub async fn custom_api_key(&self) -> AppResult<Option<String>> {
        let inner = self.read().await;
        stored_key(&inner.pool, CUSTOM_API_KEY).await
    }

    pub async fn set_custom_api_key(&self, key: Option<String>) -> AppResult<()> {
        let inner = self.read().await;
        store_key(&inner.pool, CUSTOM_API_KEY, key).await
    }
}

async fn load_settings(pool: &SqlitePool, database_dir: &Path) -> AppResult<Settings> {
    let rows =
        sqlx::query!(r#"SELECT key AS "key!: String", value AS "value!: String" FROM settings"#)
            .fetch_all(pool)
            .await
            .map_err(AppError::database)?;

    let mut fields = to_object(&Settings::default())?;
    for row in rows {
        if row.key == GROQ_API_KEY || row.key == CUSTOM_API_KEY {
            continue;
        }
        if DERIVED_FIELDS.contains(&row.key.as_str()) || !fields.contains_key(&row.key) {
            tracing::warn!(key = %row.key, "ignoring unknown setting");
            continue;
        }
        let value: Value = match serde_json::from_str(&row.value) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(key = %row.key, %error, "ignoring unreadable setting");
                continue;
            }
        };
        // Check each row on its own so one bad value only resets that field, not all settings.
        let previous = fields.insert(row.key.clone(), value);
        if let Err(error) = serde_json::from_value::<Settings>(Value::Object(fields.clone())) {
            tracing::warn!(key = %row.key, %error, "ignoring setting with an unexpected value");
            if let Some(previous) = previous {
                fields.insert(row.key, previous);
            }
        }
    }

    let mut settings: Settings =
        serde_json::from_value(Value::Object(fields)).map_err(AppError::other)?;
    settings.groq_api_key_present =
        stored_key(pool, GROQ_API_KEY).await?.is_some() || env_groq_key().is_some();
    settings.custom_api_key_present = stored_key(pool, CUSTOM_API_KEY).await?.is_some();
    settings.database_dir = database_dir.to_string_lossy().into_owned();
    Ok(settings)
}

fn to_object(settings: &Settings) -> AppResult<Map<String, Value>> {
    match serde_json::to_value(settings).map_err(AppError::other)? {
        Value::Object(fields) => Ok(fields),
        _ => Err(AppError::Other(
            "Settings did not serialize to an object".to_owned(),
        )),
    }
}

/// camelCase names of the fields the patch sets. `microphone: Some(None)` serializes as `null`
/// like an absent field, so it is checked separately.
fn patched_keys(patch: &SettingsPatch) -> AppResult<Vec<String>> {
    let Value::Object(fields) = serde_json::to_value(patch).map_err(AppError::other)? else {
        return Err(AppError::Other(
            "Settings patch did not serialize to an object".to_owned(),
        ));
    };
    let mut keys: Vec<String> = fields
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, _)| key)
        .collect();
    if patch.microphone.is_some() && !keys.iter().any(|key| key == "microphone") {
        keys.push("microphone".to_owned());
    }
    Ok(keys)
}

fn apply_patch(settings: &mut Settings, patch: SettingsPatch) {
    // Exhaustive destructuring: a new patch field fails to compile until it is handled here.
    let SettingsPatch {
        hold_shortcut,
        toggle_shortcut,
        microphone,
        languages,
        fallback_language,
        transcription_model,
        style,
        sounds,
        sound_theme,
        sound_volume,
        mute_while_dictating,
        launch_at_login,
        show_bar_always,
        post_processing,
        groq_formatting_model,
        custom_base_url,
        custom_model,
        show_in_dock,
        theme,
        restore_clipboard,
        max_recording_seconds,
        onboarding_complete,
        check_for_updates,
        skipped_update,
    } = patch;

    set_trimmed(&mut settings.hold_shortcut, hold_shortcut);
    set_trimmed(&mut settings.toggle_shortcut, toggle_shortcut);
    if let Some(microphone) = microphone {
        settings.microphone = microphone
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
    }
    if let Some(languages) = languages {
        settings.languages = languages
            .iter()
            .map(|code| code.trim().to_owned())
            .collect();
    }
    set_trimmed(&mut settings.fallback_language, fallback_language);
    set_trimmed(&mut settings.transcription_model, transcription_model);
    set(&mut settings.style, style);
    set(&mut settings.sounds, sounds);
    set(&mut settings.sound_theme, sound_theme);
    set(&mut settings.sound_volume, sound_volume);
    set(&mut settings.mute_while_dictating, mute_while_dictating);
    set(&mut settings.launch_at_login, launch_at_login);
    set(&mut settings.show_bar_always, show_bar_always);
    set(&mut settings.post_processing, post_processing);
    set_trimmed(&mut settings.groq_formatting_model, groq_formatting_model);
    set_trimmed(&mut settings.custom_base_url, custom_base_url);
    set_trimmed(&mut settings.custom_model, custom_model);
    set(&mut settings.show_in_dock, show_in_dock);
    set(&mut settings.theme, theme);
    set(&mut settings.restore_clipboard, restore_clipboard);
    set(&mut settings.max_recording_seconds, max_recording_seconds);
    set(&mut settings.onboarding_complete, onboarding_complete);
    set(&mut settings.check_for_updates, check_for_updates);
    set_trimmed(&mut settings.skipped_update, skipped_update);
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

fn set_trimmed(field: &mut String, value: Option<String>) {
    if let Some(value) = value {
        *field = value.trim().to_owned();
    }
}

fn validate(settings: &Settings) -> AppResult<()> {
    if settings.hold_shortcut.is_empty() {
        return Err(AppError::invalid(
            "The hold-to-speak shortcut can't be empty",
        ));
    }
    if settings.toggle_shortcut.is_empty() {
        return Err(AppError::invalid("The toggle shortcut can't be empty"));
    }
    if !settings.sound_volume.is_finite() || !(0.0..=1.0).contains(&settings.sound_volume) {
        return Err(AppError::invalid("Sound volume must be between 0 and 1"));
    }
    if !(MIN_RECORDING_SECONDS..=MAX_RECORDING_SECONDS).contains(&settings.max_recording_seconds) {
        return Err(AppError::invalid(
            "Maximum recording length must be between 10 seconds and 30 minutes",
        ));
    }
    if settings.languages.is_empty() {
        return Err(AppError::invalid("Choose at least one dictation language"));
    }
    if let Some(bad) = settings
        .languages
        .iter()
        .find(|code| !is_language_code(code))
    {
        return Err(AppError::invalid(format!(
            "“{bad}” is not a two-letter language code"
        )));
    }
    if !is_language_code(&settings.fallback_language) {
        return Err(AppError::invalid(format!(
            "“{}” is not a two-letter language code",
            settings.fallback_language
        )));
    }
    let url = settings.custom_base_url.as_str();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::invalid(
            "The custom endpoint URL must start with http:// or https://",
        ));
    }
    Ok(())
}

fn is_language_code(code: &str) -> bool {
    code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_lowercase())
}

fn env_groq_key() -> Option<String> {
    std::env::var(GROQ_API_KEY_ENV)
        .ok()
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty())
}

async fn stored_key(pool: &SqlitePool, name: &str) -> AppResult<Option<String>> {
    let stored = sqlx::query_scalar!(
        r#"SELECT value AS "value!: String" FROM settings WHERE key = ?1"#,
        name
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::database)?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    match serde_json::from_str::<String>(&stored) {
        Ok(key) => Ok(Some(key.trim().to_owned()).filter(|key| !key.is_empty())),
        Err(error) => {
            tracing::warn!(key = name, %error, "ignoring unreadable API key");
            Ok(None)
        }
    }
}

async fn store_key(pool: &SqlitePool, name: &str, key: Option<String>) -> AppResult<()> {
    let key = key
        .map(|key| key.trim().to_owned())
        .filter(|key| !key.is_empty());
    match key {
        Some(key) => {
            let value = serde_json::to_string(&key).map_err(AppError::other)?;
            sqlx::query!(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                name,
                value
            )
            .execute(pool)
            .await
            .map_err(AppError::database)?;
        }
        None => {
            sqlx::query!("DELETE FROM settings WHERE key = ?1", name)
                .execute(pool)
                .await
                .map_err(AppError::database)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sv_domain::{SoundTheme, Style, Theme};

    use super::*;

    async fn open() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn fresh_database_returns_defaults_with_derived_fields() {
        let (dir, db) = open().await;
        let settings = db.settings().await.unwrap();
        let expected = Settings {
            groq_api_key_present: env_groq_key().is_some(),
            database_dir: dir.path().to_string_lossy().into_owned(),
            ..Settings::default()
        };
        assert_eq!(settings, expected);
    }

    #[tokio::test]
    async fn patch_round_trips_through_a_reopen() {
        let (dir, db) = open().await;
        let patch = SettingsPatch {
            style: Some(Style::Casual),
            sound_theme: Some(SoundTheme::Chime),
            theme: Some(Theme::Dark),
            sound_volume: Some(0.55),
            microphone: Some(Some("MacBook Pro Microphone".to_owned())),
            languages: Some(vec!["en".to_owned()]),
            max_recording_seconds: Some(600),
            ..SettingsPatch::default()
        };
        let returned = db.update_settings(patch).await.unwrap();
        drop(db);

        let reopened = Db::open_at(dir.path()).await.unwrap();
        let stored = reopened.settings().await.unwrap();
        assert_eq!(stored, returned);
        assert_eq!(stored.style, Style::Casual);
        assert_eq!(stored.sound_theme, SoundTheme::Chime);
        assert_eq!(stored.theme, Theme::Dark);
        assert!((stored.sound_volume - 0.55).abs() < f32::EPSILON);
        assert_eq!(stored.microphone.as_deref(), Some("MacBook Pro Microphone"));
        assert_eq!(stored.languages, vec!["en"]);
        assert_eq!(stored.max_recording_seconds, 600);

        let reset = SettingsPatch {
            microphone: Some(None),
            ..SettingsPatch::default()
        };
        assert_eq!(
            reopened.update_settings(reset).await.unwrap().microphone,
            None
        );
        assert_eq!(reopened.settings().await.unwrap().microphone, None);
    }

    #[tokio::test]
    async fn invalid_patch_is_rejected_and_saves_nothing() {
        let (_dir, db) = open().await;
        let before = db.settings().await.unwrap();
        let invalid = [
            SettingsPatch {
                sound_volume: Some(1.5),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                max_recording_seconds: Some(5),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                languages: Some(Vec::new()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                languages: Some(vec!["EN".to_owned()]),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                fallback_language: Some("hin".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                hold_shortcut: Some("  ".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                custom_base_url: Some("localhost:11434".to_owned()),
                ..SettingsPatch::default()
            },
        ];
        for bad in invalid {
            // Pair each invalid field with a valid one to prove the valid one is not saved either.
            let patch = SettingsPatch {
                style: Some(Style::Casual),
                ..bad
            };
            let error = db.update_settings(patch).await.unwrap_err();
            assert!(matches!(error, AppError::InvalidInput(_)), "{error}");
        }
        assert_eq!(db.settings().await.unwrap(), before);
    }

    #[tokio::test]
    async fn unknown_and_malformed_rows_are_ignored() {
        let (_dir, db) = open().await;
        {
            let inner = db.read().await;
            for (key, value) in [
                ("noSuchSetting", "true"),
                ("soundVolume", "\"loud\""),
                ("sounds", "not json"),
                ("databaseDir", "\"/tmp\""),
                ("style", "\"casual\""),
            ] {
                sqlx::query!(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                    key,
                    value
                )
                .execute(&inner.pool)
                .await
                .unwrap();
            }
        }
        let settings = db.settings().await.unwrap();
        let defaults = Settings::default();
        assert_eq!(settings.style, Style::Casual);
        assert!((settings.sound_volume - defaults.sound_volume).abs() < f32::EPSILON);
        assert_eq!(settings.sounds, defaults.sounds);
        assert_ne!(settings.database_dir, "/tmp");
    }

    #[tokio::test]
    async fn api_keys_never_appear_in_settings() {
        let (_dir, db) = open().await;
        assert!(!db.settings().await.unwrap().custom_api_key_present);

        db.set_custom_api_key(Some(" sk-custom-secret ".to_owned()))
            .await
            .unwrap();
        db.set_groq_api_key(Some("gsk-groq-secret".to_owned()))
            .await
            .unwrap();
        let settings = db.settings().await.unwrap();
        assert!(settings.custom_api_key_present);
        assert!(settings.groq_api_key_present);
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("sk-custom-secret") && !json.contains("gsk-groq-secret"));
        assert_eq!(
            db.custom_api_key().await.unwrap().as_deref(),
            Some("sk-custom-secret")
        );
        assert_eq!(
            db.groq_api_key().await.unwrap().as_deref(),
            Some("gsk-groq-secret")
        );

        db.set_custom_api_key(None).await.unwrap();
        assert!(!db.settings().await.unwrap().custom_api_key_present);
        assert_eq!(db.custom_api_key().await.unwrap(), None);
        db.set_custom_api_key(Some("   ".to_owned())).await.unwrap();
        assert_eq!(db.custom_api_key().await.unwrap(), None);
    }
}
