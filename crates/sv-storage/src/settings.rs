// FilePath: crates/sv-storage/src/settings.rs
//! Settings persistence: one row per field, keyed by the camelCase field name, value as JSON.
//! Missing rows take `Settings::default()`, so adding a setting never needs a migration. The
//! last-run version lives in the same table under its own key and never enters `Settings`.

use std::path::Path;

use serde_json::{Map, Value};
use sqlx::SqlitePool;
use sv_domain::languages::language;
use sv_domain::models::{LOCAL_LLM, LOCAL_TRANSCRIPTION_MODELS, SMART_SELECT};
use sv_domain::settings::FN_KEY_ACCELERATOR;
use sv_domain::{AppError, AppResult, AppStyle, Settings, SettingsPatch, VocabularyPack};

use crate::db::Db;

/// The app version that ran last; never part of `Settings`.
const LAST_RUN_VERSION: &str = "lastRunVersion";
/// Computed on every read; a stored row with one of these names is ignored.
const DERIVED_FIELDS: [&str; 1] = ["databaseDir"];
/// Most per-app style overrides and excluded context apps kept.
const MAX_APP_ENTRIES: usize = 200;
const MIN_RECORDING_SECONDS: u32 = 10;
const MAX_RECORDING_SECONDS: u32 = 1800;
const MIN_PASTED_TEXT_SECONDS: u32 = 1;
const MAX_PASTED_TEXT_SECONDS: u32 = 10;

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

    /// Stores `version` as the one that ran last and returns the one stored before, `None` on
    /// the first launch that records it.
    pub async fn record_run_version(&self, version: &str) -> AppResult<Option<String>> {
        let inner = self.read().await;
        let previous = stored_key(&inner.pool, LAST_RUN_VERSION).await?;
        store_key(&inner.pool, LAST_RUN_VERSION, Some(version.to_owned())).await?;
        Ok(previous)
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
        if row.key == LAST_RUN_VERSION {
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
    settings.database_dir = database_dir.to_string_lossy().into_owned();
    // The default edit shortcut can collide with a dictation shortcut chosen before edit mode
    // existed. Edit mode then starts off instead of every later settings save failing.
    if validate_edit_shortcut(&settings).is_err() {
        settings.edit_shortcut.clear();
    }
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
        edit_shortcut,
        escape_cancels,
        microphone,
        languages,
        fallback_language,
        transcription_model,
        smart_retry,
        style,
        category_styles,
        app_styles,
        sounds,
        sound_theme,
        sound_volume,
        mute_while_dictating,
        launch_at_login,
        show_bar_always,
        pasted_text_seconds,
        post_processing,
        local_llm_model,
        meeting_assistant,
        foreground_context,
        ide_context,
        context_excluded_apps,
        live_transcript,
        meeting_system_audio,
        translation_language,
        show_in_dock,
        theme,
        restore_clipboard,
        max_recording_seconds,
        onboarding_complete,
        check_for_updates,
        skipped_update,
        dictionary_sort,
        learn_from_edits,
        vocabulary_packs,
    } = patch;

    set_trimmed(&mut settings.hold_shortcut, hold_shortcut);
    set_trimmed(&mut settings.toggle_shortcut, toggle_shortcut);
    set_trimmed(&mut settings.edit_shortcut, edit_shortcut);
    set(&mut settings.escape_cancels, escape_cancels);
    if let Some(microphone) = microphone {
        settings.microphone = microphone
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty());
    }
    // Stored as the registry's canonical tag so every reader can compare tags exactly.
    if let Some(languages) = languages {
        settings.languages = languages.iter().map(|tag| canonical_tag(tag)).collect();
    }
    if let Some(fallback) = fallback_language {
        settings.fallback_language = canonical_tag(&fallback);
    }
    set_trimmed(&mut settings.transcription_model, transcription_model);
    set(&mut settings.smart_retry, smart_retry);
    set(&mut settings.style, style);
    set(&mut settings.category_styles, category_styles);
    if let Some(apps) = app_styles {
        settings.app_styles = clean_app_styles(apps);
    }
    set(&mut settings.sounds, sounds);
    set(&mut settings.sound_theme, sound_theme);
    set(&mut settings.sound_volume, sound_volume);
    set(&mut settings.mute_while_dictating, mute_while_dictating);
    set(&mut settings.launch_at_login, launch_at_login);
    set(&mut settings.show_bar_always, show_bar_always);
    set(&mut settings.pasted_text_seconds, pasted_text_seconds);
    set(&mut settings.post_processing, post_processing);
    set_trimmed(&mut settings.local_llm_model, local_llm_model);
    set(&mut settings.meeting_assistant, meeting_assistant);
    set(&mut settings.foreground_context, foreground_context);
    set(&mut settings.ide_context, ide_context);
    if let Some(apps) = context_excluded_apps {
        settings.context_excluded_apps = clean_bundle_ids(apps);
    }
    set(&mut settings.live_transcript, live_transcript);
    set(&mut settings.meeting_system_audio, meeting_system_audio);
    if let Some(tag) = translation_language {
        settings.translation_language = canonical_tag(&tag);
    }
    set(&mut settings.show_in_dock, show_in_dock);
    set(&mut settings.theme, theme);
    set(&mut settings.restore_clipboard, restore_clipboard);
    set(&mut settings.max_recording_seconds, max_recording_seconds);
    set(&mut settings.onboarding_complete, onboarding_complete);
    set(&mut settings.check_for_updates, check_for_updates);
    set_trimmed(&mut settings.skipped_update, skipped_update);
    set(&mut settings.dictionary_sort, dictionary_sort);
    set(&mut settings.learn_from_edits, learn_from_edits);
    if let Some(packs) = vocabulary_packs {
        // One canonical order, so the prompt's pack priority never depends on click order.
        settings.vocabulary_packs = VocabularyPack::ALL
            .into_iter()
            .filter(|pack| packs.contains(pack))
            .collect();
    }
}

/// Trimmed, without blank bundle ids; a later entry for the same app (ignoring case) replaces
/// the earlier one, so the list stays unique.
fn clean_app_styles(apps: Vec<AppStyle>) -> Vec<AppStyle> {
    let mut cleaned: Vec<AppStyle> = Vec::with_capacity(apps.len());
    for app in apps {
        let bundle_id = app.bundle_id.trim().to_owned();
        if bundle_id.is_empty() {
            continue;
        }
        let app = AppStyle {
            app_name: app.app_name.trim().to_owned(),
            bundle_id,
            style: app.style,
        };
        match cleaned
            .iter_mut()
            .find(|kept| kept.bundle_id.eq_ignore_ascii_case(&app.bundle_id))
        {
            Some(kept) => *kept = app,
            None => cleaned.push(app),
        }
    }
    cleaned
}

/// Trimmed, non-blank and unique ignoring case, first spelling kept.
fn clean_bundle_ids(ids: Vec<String>) -> Vec<String> {
    let mut cleaned: Vec<String> = Vec::with_capacity(ids.len());
    for id in ids {
        let id = id.trim();
        if !id.is_empty() && !cleaned.iter().any(|kept| kept.eq_ignore_ascii_case(id)) {
            cleaned.push(id.to_owned());
        }
    }
    cleaned
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
    validate_edit_shortcut(settings)?;
    if !settings.sound_volume.is_finite() || !(0.0..=1.0).contains(&settings.sound_volume) {
        return Err(AppError::invalid("Sound volume must be between 0 and 1"));
    }
    if !(MIN_RECORDING_SECONDS..=MAX_RECORDING_SECONDS).contains(&settings.max_recording_seconds) {
        return Err(AppError::invalid(
            "Maximum recording length must be between 10 seconds and 30 minutes",
        ));
    }
    if !(MIN_PASTED_TEXT_SECONDS..=MAX_PASTED_TEXT_SECONDS).contains(&settings.pasted_text_seconds)
    {
        return Err(AppError::invalid(
            "The pasted text must show for between 1 and 10 seconds",
        ));
    }
    if settings.languages.is_empty() {
        return Err(AppError::invalid("Choose at least one dictation language"));
    }
    if let Some(bad) = settings
        .languages
        .iter()
        .find(|tag| language(tag).is_none())
    {
        return Err(AppError::invalid(format!(
            "“{bad}” is not a supported language"
        )));
    }
    // Recognition is pinned to the fallback, and models only know base languages.
    let fallback_ok = language(&settings.fallback_language).is_some_and(|lang| !lang.is_variant());
    if !fallback_ok {
        return Err(AppError::invalid(format!(
            "“{}” can't be the fallback language",
            settings.fallback_language
        )));
    }
    let model = settings.transcription_model.as_str();
    if model != SMART_SELECT && !LOCAL_TRANSCRIPTION_MODELS.contains(&model) {
        return Err(AppError::invalid(format!(
            "“{model}” is not an on-device voice model"
        )));
    }
    if settings.local_llm_model != LOCAL_LLM {
        return Err(AppError::invalid(format!(
            "“{}” is not an on-device language model",
            settings.local_llm_model
        )));
    }
    if language(&settings.translation_language).is_none() {
        return Err(AppError::invalid(format!(
            "“{}” is not a supported translation language",
            settings.translation_language
        )));
    }
    let bad_id = settings
        .app_styles
        .iter()
        .map(|app| app.bundle_id.as_str())
        .chain(settings.context_excluded_apps.iter().map(String::as_str))
        .find(|id| !is_bundle_id(id));
    if let Some(bad) = bad_id {
        return Err(AppError::invalid(format!(
            "“{bad}” is not an app bundle ID (like com.apple.mail)"
        )));
    }
    if settings.app_styles.len() > MAX_APP_ENTRIES
        || settings.context_excluded_apps.len() > MAX_APP_ENTRIES
    {
        return Err(AppError::invalid(format!(
            "Keep at most {MAX_APP_ENTRIES} apps in a list"
        )));
    }
    Ok(())
}

/// macOS bundle ID syntax (CFBundleIdentifier): letters, digits, hyphens, underscores and
/// periods, in non-empty dot-separated parts, at most 255 characters. An ID that breaks it can
/// never match an app, so it is refused instead of saved.
fn is_bundle_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 255
        && id.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

/// Edit mode toggles on its own key combination: the Fn key is taken by dictation's watch,
/// and a combination shared with a dictation shortcut could never tell the two apart.
fn validate_edit_shortcut(settings: &Settings) -> AppResult<()> {
    let edit = settings.edit_shortcut.as_str();
    if edit.is_empty() {
        return Ok(());
    }
    if edit.eq_ignore_ascii_case(FN_KEY_ACCELERATOR) {
        return Err(AppError::invalid(
            "Fn can't be the edit shortcut; choose a key combination",
        ));
    }
    let shared = [&settings.hold_shortcut, &settings.toggle_shortcut]
        .into_iter()
        .any(|dictation| dictation.eq_ignore_ascii_case(edit));
    if shared {
        return Err(AppError::invalid(format!(
            "{edit} is already a dictation shortcut; choose another for editing"
        )));
    }
    Ok(())
}

/// The registry's spelling of `tag`; unknown tags are kept (trimmed) for validation to reject.
fn canonical_tag(tag: &str) -> String {
    language(tag).map_or_else(|| tag.trim().to_owned(), |known| known.tag.to_owned())
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
            tracing::warn!(key = name, %error, "ignoring an unreadable stored value");
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
            languages: Some(vec!["en".to_owned(), " HI-latn".to_owned()]),
            fallback_language: Some("EN".to_owned()),
            max_recording_seconds: Some(600),
            pasted_text_seconds: Some(5),
            learn_from_edits: Some(true),
            escape_cancels: Some(true),
            smart_retry: Some(false),
            transcription_model: Some(sv_domain::models::WHISPER_HINGLISH.to_owned()),
            vocabulary_packs: Some(vec![
                VocabularyPack::Sales,
                VocabularyPack::Everyday,
                VocabularyPack::Sales,
            ]),
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
        assert_eq!(stored.languages, vec!["en", "hi-Latn"]);
        assert_eq!(stored.fallback_language, "en");
        assert_eq!(stored.max_recording_seconds, 600);
        assert_eq!(stored.pasted_text_seconds, 5);
        assert!(stored.learn_from_edits);
        assert!(stored.escape_cancels);
        assert!(!stored.smart_retry);
        assert_eq!(stored.transcription_model, "whisper-hinglish");
        // Deduplicated and in the registry's order, whatever order the clicks came in.
        assert_eq!(
            stored.vocabulary_packs,
            vec![VocabularyPack::Everyday, VocabularyPack::Sales]
        );

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
                pasted_text_seconds: Some(0),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                pasted_text_seconds: Some(11),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                languages: Some(Vec::new()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                languages: Some(vec!["english".to_owned()]),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                fallback_language: Some("hin".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                fallback_language: Some("hi-Latn".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                languages: Some(vec!["en".to_owned(), "hinglish".to_owned()]),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                hold_shortcut: Some("  ".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                edit_shortcut: Some("fn".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                edit_shortcut: Some("control+slash".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                transcription_model: Some("groq-whisper".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                local_llm_model: Some("gpt-4o".to_owned()),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                translation_language: Some("klingon".to_owned()),
                ..SettingsPatch::default()
            },
            // Seen in desktop QA: saved, and could never match an app.
            SettingsPatch {
                app_styles: Some(vec![AppStyle {
                    bundle_id: "not a bundle id!".to_owned(),
                    app_name: "Nothing".to_owned(),
                    style: Style::Casual,
                }]),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                context_excluded_apps: Some(vec!["com..apple".to_owned()]),
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
    async fn default_edit_shortcut_colliding_with_a_dictation_shortcut_starts_off() {
        let (_dir, db) = open().await;
        let collide = SettingsPatch {
            toggle_shortcut: Some(sv_domain::settings::DEFAULT_EDIT_SHORTCUT.to_owned()),
            edit_shortcut: Some(String::new()),
            ..SettingsPatch::default()
        };
        db.update_settings(collide).await.unwrap();
        {
            // A database from before edit mode has no edit shortcut row at all.
            let inner = db.read().await;
            sqlx::query!("DELETE FROM settings WHERE key = 'editShortcut'")
                .execute(&inner.pool)
                .await
                .unwrap();
        }
        assert_eq!(db.settings().await.unwrap().edit_shortcut, "");
        let later = SettingsPatch {
            style: Some(Style::Casual),
            ..SettingsPatch::default()
        };
        assert_eq!(
            db.update_settings(later).await.unwrap().style,
            Style::Casual
        );
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
    async fn per_app_styles_and_context_lists_are_cleaned_and_persist() {
        use sv_domain::{AppCategory, LlmBackend, PostProcessing};

        let (dir, db) = open().await;
        let patch = SettingsPatch {
            category_styles: Some([(AppCategory::Email, Style::Formal)].into_iter().collect()),
            app_styles: Some(vec![
                AppStyle {
                    bundle_id: " md.obsidian ".to_owned(),
                    app_name: "Obsidian".to_owned(),
                    style: Style::Formal,
                },
                AppStyle {
                    bundle_id: "   ".to_owned(),
                    app_name: "Nothing".to_owned(),
                    style: Style::Formal,
                },
                // The same app again (other case): the later choice wins, still one entry.
                AppStyle {
                    bundle_id: "MD.Obsidian".to_owned(),
                    app_name: "Obsidian".to_owned(),
                    style: Style::Casual,
                },
            ]),
            context_excluded_apps: Some(vec![
                " com.bitwarden.desktop ".to_owned(),
                "COM.BITWARDEN.DESKTOP".to_owned(),
                String::new(),
            ]),
            foreground_context: Some(true),
            ide_context: Some(true),
            post_processing: Some(PostProcessing::Local),
            meeting_assistant: Some(LlmBackend::Apple),
            translation_language: Some(" DE ".to_owned()),
            ..SettingsPatch::default()
        };
        db.update_settings(patch).await.unwrap();
        drop(db);

        let stored = Db::open_at(dir.path())
            .await
            .unwrap()
            .settings()
            .await
            .unwrap();
        assert_eq!(
            stored.category_styles.get(&AppCategory::Email),
            Some(&Style::Formal)
        );
        assert_eq!(stored.app_styles.len(), 1);
        assert_eq!(stored.app_styles[0].bundle_id, "MD.Obsidian");
        assert_eq!(stored.app_styles[0].style, Style::Casual);
        assert_eq!(stored.context_excluded_apps, vec!["com.bitwarden.desktop"]);
        assert!(stored.foreground_context && stored.ide_context);
        assert_eq!(stored.post_processing, PostProcessing::Local);
        assert_eq!(stored.meeting_assistant, LlmBackend::Apple);
        assert_eq!(stored.translation_language, "de");
    }

    #[test]
    fn real_bundle_ids_pass_and_impossible_ones_do_not() {
        for id in [
            "com.apple.mail",
            "com.todesktop.230313mzl4w4u92",
            "org.whispersystems.signal-desktop",
            "com.jetbrains.rustrover",
            "dev.warp.Warp-Stable",
            "com.microsoft.VSCode",
            "net.whatsapp.WhatsApp",
            "com.example.app_beta",
            "Finder",
        ] {
            assert!(is_bundle_id(id), "{id} refused");
        }
        for id in [
            "",
            "not a bundle id!",
            "com..apple",
            ".com.apple",
            "com.apple.",
            "ä.b",
        ] {
            assert!(!is_bundle_id(id), "{id:?} accepted");
        }
    }

    #[tokio::test]
    async fn run_version_returns_the_previous_launch_and_stays_out_of_settings() {
        let (dir, db) = open().await;
        let before = db.settings().await.unwrap();
        assert_eq!(db.record_run_version("0.0.9").await.unwrap(), None);
        let reopened = Db::open_at(dir.path()).await.unwrap();
        assert_eq!(
            reopened
                .record_run_version("0.0.10")
                .await
                .unwrap()
                .as_deref(),
            Some("0.0.9")
        );
        assert_eq!(
            reopened
                .record_run_version("0.0.10")
                .await
                .unwrap()
                .as_deref(),
            Some("0.0.10")
        );
        assert_eq!(reopened.settings().await.unwrap(), before);
    }
}
