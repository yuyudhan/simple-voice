// FilePath: crates/sv-domain/src/settings.rs
//! User settings. Persisted one row per field in the `settings` table (JSON values) by
//! sv-storage; fields missing from the table take the defaults below, so adding a setting never
//! needs a data migration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// Caps and full punctuation.
    #[default]
    Formal,
    /// Caps, lighter punctuation (no trailing period on short messages).
    Casual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    #[default]
    Soft,
    Glass,
    Pop,
    Chime,
}

/// Appearance. `System` follows the macOS light/dark setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Which backend runs the LLM formatting pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessing {
    /// Groq chat completions (remote).
    #[default]
    Groq,
    /// Apple Intelligence Foundation Models (on-device, macOS 26+).
    Apple,
    /// Any OpenAI-compatible `/chat/completions` endpoint (Ollama, LM Studio, remote services).
    Custom,
    /// Deterministic formatting only.
    Off,
}

/// Order of the Dictionary page list. Remembered so the page opens the way it was left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictionarySort {
    #[default]
    NameAsc,
    NameDesc,
    /// Most recently added first.
    Newest,
    Oldest,
}

/// The Fn (Globe) key on its own. Carbon hot keys cannot register a lone modifier, so the app
/// watches this key through the engine helper instead of the global-shortcut plugin.
pub const FN_KEY_ACCELERATOR: &str = "Fn";
pub const DEFAULT_HOLD_SHORTCUT: &str = FN_KEY_ACCELERATOR;
pub const DEFAULT_TOGGLE_SHORTCUT: &str = "Control+Slash";
/// Hold-to-edit: rewrites the selected text by voice. An empty accelerator turns edit mode off.
pub const DEFAULT_EDIT_SHORTCUT: &str = "Alt+Slash";
pub const DEFAULT_TRANSCRIPTION_MODEL: &str = "groq-whisper";
pub const DEFAULT_GROQ_FORMATTING_MODEL: &str = "qwen/qwen3.8-27b";
pub const DEFAULT_CUSTOM_BASE_URL: &str = "http://localhost:11434/v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub hold_shortcut: String,
    pub toggle_shortcut: String,
    /// Hold to edit the selected text by voice; empty = edit mode off. Never `Fn`.
    pub edit_shortcut: String,
    /// Input device name; `None` = automatic (built-in microphone preferred).
    pub microphone: Option<String>,
    /// Allowed dictation languages, ISO 639-1.
    pub languages: Vec<String>,
    /// Language a transcription is re-run in when detection lands outside `languages`.
    pub fallback_language: String,
    pub transcription_model: String,
    pub style: Style,
    /// Derived: a Groq key is stored (or `GROQ_API_KEY` is set). Never deserialized from rows.
    pub groq_api_key_present: bool,
    pub sounds: bool,
    pub sound_theme: SoundTheme,
    /// 0.0..=1.0
    pub sound_volume: f32,
    pub mute_while_dictating: bool,
    pub launch_at_login: bool,
    pub show_bar_always: bool,
    pub post_processing: PostProcessing,
    pub groq_formatting_model: String,
    pub custom_base_url: String,
    pub custom_model: String,
    /// Derived: a key for the custom endpoint is stored.
    pub custom_api_key_present: bool,
    pub show_in_dock: bool,
    pub theme: Theme,
    pub restore_clipboard: bool,
    pub max_recording_seconds: u32,
    pub onboarding_complete: bool,
    /// Ask GitHub once a day whether a newer release is out.
    pub check_for_updates: bool,
    /// Release version whose banner the user dismissed; empty = none. A newer one shows again.
    pub skipped_update: String,
    pub dictionary_sort: DictionarySort,
    /// Watch the text field after a paste and add the words the user corrects to the dictionary.
    pub learn_from_edits: bool,
    /// Derived: directory currently holding the database.
    pub database_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hold_shortcut: DEFAULT_HOLD_SHORTCUT.to_owned(),
            toggle_shortcut: DEFAULT_TOGGLE_SHORTCUT.to_owned(),
            edit_shortcut: DEFAULT_EDIT_SHORTCUT.to_owned(),
            microphone: None,
            languages: vec!["en".to_owned(), "hi".to_owned()],
            fallback_language: "hi".to_owned(),
            transcription_model: DEFAULT_TRANSCRIPTION_MODEL.to_owned(),
            style: Style::Formal,
            groq_api_key_present: false,
            sounds: true,
            sound_theme: SoundTheme::Soft,
            sound_volume: 0.3,
            mute_while_dictating: false,
            launch_at_login: false,
            show_bar_always: false,
            post_processing: PostProcessing::Groq,
            groq_formatting_model: DEFAULT_GROQ_FORMATTING_MODEL.to_owned(),
            custom_base_url: DEFAULT_CUSTOM_BASE_URL.to_owned(),
            custom_model: String::new(),
            custom_api_key_present: false,
            show_in_dock: true,
            theme: Theme::System,
            // Off by default: the dictated text stays on the clipboard, so it can be pasted
            // again or by hand when the automatic paste could not reach the app.
            restore_clipboard: false,
            max_recording_seconds: 300,
            onboarding_complete: false,
            check_for_updates: true,
            skipped_update: String::new(),
            dictionary_sort: DictionarySort::NameAsc,
            // Off by default: learning reads the focused text field after every paste.
            learn_from_edits: false,
            database_dir: String::new(),
        }
    }
}

/// Partial update from the UI. `None` = leave unchanged. Derived fields are absent on purpose.
/// `microphone` is doubly optional: `Some(None)` switches back to automatic selection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsPatch {
    pub hold_shortcut: Option<String>,
    pub toggle_shortcut: Option<String>,
    pub edit_shortcut: Option<String>,
    #[serde(with = "double_option")]
    pub microphone: Option<Option<String>>,
    pub languages: Option<Vec<String>>,
    pub fallback_language: Option<String>,
    pub transcription_model: Option<String>,
    pub style: Option<Style>,
    pub sounds: Option<bool>,
    pub sound_theme: Option<SoundTheme>,
    pub sound_volume: Option<f32>,
    pub mute_while_dictating: Option<bool>,
    pub launch_at_login: Option<bool>,
    pub show_bar_always: Option<bool>,
    pub post_processing: Option<PostProcessing>,
    pub groq_formatting_model: Option<String>,
    pub custom_base_url: Option<String>,
    pub custom_model: Option<String>,
    pub show_in_dock: Option<bool>,
    pub theme: Option<Theme>,
    pub restore_clipboard: Option<bool>,
    pub max_recording_seconds: Option<u32>,
    pub onboarding_complete: Option<bool>,
    pub check_for_updates: Option<bool>,
    pub skipped_update: Option<String>,
    pub dictionary_sort: Option<DictionarySort>,
    pub learn_from_edits: Option<bool>,
}

/// Distinguishes an absent key (`None`) from an explicit `null` (`Some(None)`).
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(
        value: &Option<Option<String>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            Some(inner) => inner.serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Option<String>>, D::Error> {
        Option::<String>::deserialize(deserializer).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_distinguishes_missing_from_null_microphone() {
        let missing: Result<SettingsPatch, _> = serde_json::from_str("{}");
        let null: Result<SettingsPatch, _> = serde_json::from_str(r#"{"microphone":null}"#);
        let named: Result<SettingsPatch, _> =
            serde_json::from_str(r#"{"microphone":"MacBook Pro Microphone"}"#);
        assert_eq!(missing.ok().map(|p| p.microphone), Some(None));
        assert_eq!(null.ok().map(|p| p.microphone), Some(Some(None)));
        assert_eq!(
            named.ok().map(|p| p.microphone),
            Some(Some(Some("MacBook Pro Microphone".to_owned())))
        );
    }

    #[test]
    fn enums_use_the_wire_names_the_ui_sends() {
        let patch: Result<SettingsPatch, _> = serde_json::from_str(
            r#"{"style":"casual","soundTheme":"chime","postProcessing":"apple","theme":"dark",
                "dictionarySort":"name_desc"}"#,
        );
        let patch = patch.ok();
        assert_eq!(patch.as_ref().and_then(|p| p.style), Some(Style::Casual));
        assert_eq!(
            patch.as_ref().and_then(|p| p.sound_theme),
            Some(SoundTheme::Chime)
        );
        assert_eq!(patch.as_ref().and_then(|p| p.theme), Some(Theme::Dark));
        assert_eq!(
            patch.as_ref().and_then(|p| p.dictionary_sort),
            Some(DictionarySort::NameDesc)
        );
        assert_eq!(
            patch.and_then(|p| p.post_processing),
            Some(PostProcessing::Apple)
        );
    }
}
