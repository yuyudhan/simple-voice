// FilePath: crates/sv-domain/src/settings.rs
//! User settings. Persisted one row per field in the `settings` table (JSON values) by
//! sv-storage; fields missing from the table take the defaults below, so adding a setting never
//! needs a data migration.

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::apps::{categorize, AppCategory};
use crate::vocabulary_packs::{default_vocabulary_packs, VocabularyPack};

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

/// Which on-device backend runs the LLM formatting pass. Nothing leaves the Mac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessing {
    /// Apple Intelligence Foundation Models (on-device, macOS 26+).
    #[default]
    Apple,
    /// The downloaded local language model (`Settings::local_llm_model`).
    Local,
    /// Deterministic formatting only.
    Off,
}

/// Which on-device language model answers meeting summaries, chat and translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmBackend {
    /// Apple Intelligence Foundation Models (on-device, macOS 26+).
    Apple,
    /// The downloaded local language model (`Settings::local_llm_model`).
    #[default]
    Local,
}

/// A per-app writing style: wins over the app's category style and the global style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStyle {
    pub bundle_id: String,
    /// Display name remembered when the override was made.
    pub app_name: String,
    pub style: Style,
}

/// Where the style a dictation used came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StyleSource {
    /// An explicit bundle-id override in `Settings::app_styles`.
    App,
    /// The style chosen for the app's category in `Settings::category_styles`.
    Category,
    /// `Settings::style`.
    Global,
}

impl StyleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Category => "category",
            Self::Global => "global",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "app" => Some(Self::App),
            "category" => Some(Self::Category),
            "global" => Some(Self::Global),
            _ => None,
        }
    }
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
/// Toggle-to-edit: rewrites the selected text by voice. An empty accelerator turns edit mode off.
pub const DEFAULT_EDIT_SHORTCUT: &str = "Alt+Slash";
/// Runs on the Mac, so a new install needs no account; onboarding prompts for its download.
pub const DEFAULT_TRANSCRIPTION_MODEL: &str = crate::models::PARAKEET_TDT_V3;
/// Password managers: their fields never feed dictation context.
pub const DEFAULT_CONTEXT_EXCLUDED_APPS: &[&str] = &[
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.bitwarden.desktop",
    "com.apple.keychainaccess",
    "com.apple.Passwords",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub hold_shortcut: String,
    pub toggle_shortcut: String,
    /// Toggle to edit the selected text by voice; empty = edit mode off. Never `Fn`.
    pub edit_shortcut: String,
    /// Esc cancels a recording in progress. Registering Esc globally takes it from every app
    /// while recording, so it is opt-in.
    pub escape_cancels: bool,
    /// Input device name; `None` = automatic (built-in microphone preferred).
    pub microphone: Option<String>,
    /// Allowed dictation languages: tags of [`crate::languages::LANGUAGES`].
    pub languages: Vec<String>,
    /// Language a transcription is re-run in when detection lands outside `languages`. Always
    /// a registry tag models know as is, never a script variant such as `hi-Latn`.
    pub fallback_language: String,
    pub transcription_model: String,
    /// Smart Select double-checks a low-confidence result with a second model; when off, only
    /// an error makes it try another model.
    pub smart_retry: bool,
    /// The global style; `category_styles` and `app_styles` override it per app.
    pub style: Style,
    /// Style per app category; a category without an entry uses `style`.
    pub category_styles: BTreeMap<AppCategory, Style>,
    /// Style per app (bundle id, unique ignoring case); wins over `category_styles`.
    pub app_styles: Vec<AppStyle>,
    pub sounds: bool,
    pub sound_theme: SoundTheme,
    /// 0.0..=1.0
    pub sound_volume: f32,
    pub mute_while_dictating: bool,
    pub launch_at_login: bool,
    pub show_bar_always: bool,
    /// How long the pill shows the start of the pasted text after a dictation.
    pub pasted_text_seconds: u32,
    pub post_processing: PostProcessing,
    /// Catalog id of the downloadable local language model (`PostProcessing::Local`,
    /// `LlmBackend::Local`).
    pub local_llm_model: String,
    /// Model for meeting summaries, chat and translation.
    pub meeting_assistant: LlmBackend,
    /// Read the focused text field (bounded, on-device) at recording start to prime
    /// transcription and formatting with the names around the caret.
    pub foreground_context: bool,
    /// In code editors, also feed file names and identifiers visible in the focused editor.
    pub ide_context: bool,
    /// Bundle ids whose fields are never read for context.
    pub context_excluded_apps: Vec<String>,
    /// Show the live transcript while recording when the voice model streams.
    pub live_transcript: bool,
    /// The last choice for recording system audio in meetings (asked again on every start).
    pub meeting_system_audio: bool,
    /// Registry tag meetings were last translated into.
    pub translation_language: String,
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
    /// Built-in vocabulary packs used beside the personal dictionary; stored without
    /// duplicates in [`VocabularyPack::ALL`] order. Empty turns built-in vocabulary off.
    pub vocabulary_packs: Vec<VocabularyPack>,
    /// Derived: directory currently holding the database.
    pub database_dir: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hold_shortcut: DEFAULT_HOLD_SHORTCUT.to_owned(),
            toggle_shortcut: DEFAULT_TOGGLE_SHORTCUT.to_owned(),
            edit_shortcut: DEFAULT_EDIT_SHORTCUT.to_owned(),
            // Off by default: Esc belongs to the focused app (Vim, dialogs) unless the user opts in.
            escape_cancels: false,
            microphone: None,
            languages: vec!["en".to_owned(), "hi".to_owned(), "hi-Latn".to_owned()],
            fallback_language: "hi".to_owned(),
            transcription_model: DEFAULT_TRANSCRIPTION_MODEL.to_owned(),
            smart_retry: true,
            style: Style::Formal,
            category_styles: BTreeMap::new(),
            app_styles: Vec::new(),
            sounds: true,
            sound_theme: SoundTheme::Soft,
            sound_volume: 0.3,
            mute_while_dictating: false,
            launch_at_login: false,
            show_bar_always: false,
            pasted_text_seconds: 2,
            post_processing: PostProcessing::Apple,
            local_llm_model: crate::models::LOCAL_LLM.to_owned(),
            meeting_assistant: LlmBackend::Local,
            // Off by default: context reads other apps' text.
            foreground_context: false,
            ide_context: false,
            context_excluded_apps: DEFAULT_CONTEXT_EXCLUDED_APPS
                .iter()
                .map(|id| (*id).to_owned())
                .collect(),
            live_transcript: true,
            meeting_system_audio: false,
            translation_language: "en".to_owned(),
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
            vocabulary_packs: default_vocabulary_packs(),
            database_dir: String::new(),
        }
    }
}

impl Settings {
    /// The style a dictation into this app uses, decided at recording start: an explicit
    /// bundle-id override, else the app category's style, else the global style.
    pub fn style_for(
        &self,
        bundle_id: Option<&str>,
        app_name: Option<&str>,
    ) -> (Style, StyleSource) {
        if let Some(id) = bundle_id.map(str::trim).filter(|id| !id.is_empty()) {
            if let Some(app) = self
                .app_styles
                .iter()
                .find(|app| app.bundle_id.eq_ignore_ascii_case(id))
            {
                return (app.style, StyleSource::App);
            }
        }
        let category = categorize(bundle_id, app_name);
        match self.category_styles.get(&category) {
            Some(style) => (*style, StyleSource::Category),
            None => (self.style, StyleSource::Global),
        }
    }

    /// Whether context may be read from this app: the feature is on and the app is not excluded.
    pub fn context_allowed(&self, bundle_id: Option<&str>) -> bool {
        self.foreground_context
            && !bundle_id.is_some_and(|id| {
                self.context_excluded_apps
                    .iter()
                    .any(|excluded| excluded.eq_ignore_ascii_case(id.trim()))
            })
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
    pub escape_cancels: Option<bool>,
    #[serde(with = "double_option")]
    pub microphone: Option<Option<String>>,
    pub languages: Option<Vec<String>>,
    pub fallback_language: Option<String>,
    pub transcription_model: Option<String>,
    pub smart_retry: Option<bool>,
    pub style: Option<Style>,
    pub category_styles: Option<BTreeMap<AppCategory, Style>>,
    pub app_styles: Option<Vec<AppStyle>>,
    pub sounds: Option<bool>,
    pub sound_theme: Option<SoundTheme>,
    pub sound_volume: Option<f32>,
    pub mute_while_dictating: Option<bool>,
    pub launch_at_login: Option<bool>,
    pub show_bar_always: Option<bool>,
    pub pasted_text_seconds: Option<u32>,
    pub post_processing: Option<PostProcessing>,
    pub local_llm_model: Option<String>,
    pub meeting_assistant: Option<LlmBackend>,
    pub foreground_context: Option<bool>,
    pub ide_context: Option<bool>,
    pub context_excluded_apps: Option<Vec<String>>,
    pub live_transcript: Option<bool>,
    pub meeting_system_audio: Option<bool>,
    pub translation_language: Option<String>,
    pub show_in_dock: Option<bool>,
    pub theme: Option<Theme>,
    pub restore_clipboard: Option<bool>,
    pub max_recording_seconds: Option<u32>,
    pub onboarding_complete: Option<bool>,
    pub check_for_updates: Option<bool>,
    pub skipped_update: Option<String>,
    pub dictionary_sort: Option<DictionarySort>,
    pub learn_from_edits: Option<bool>,
    pub vocabulary_packs: Option<Vec<VocabularyPack>>,
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

    fn with_styles() -> Settings {
        let mut settings = Settings {
            style: Style::Formal,
            ..Settings::default()
        };
        settings
            .category_styles
            .insert(AppCategory::WorkMessages, Style::Casual);
        settings.app_styles.push(AppStyle {
            bundle_id: "com.tinyspeck.slackmacgap".to_owned(),
            app_name: "Slack".to_owned(),
            style: Style::Formal,
        });
        settings
    }

    #[test]
    fn an_app_override_beats_its_category_which_beats_the_global_style() {
        let settings = with_styles();
        // Slack is a work-messages app (casual) but has its own formal override.
        assert_eq!(
            settings.style_for(Some("COM.TinySpeck.SlackMacGap"), Some("Slack")),
            (Style::Formal, StyleSource::App)
        );
        // Teams shares Slack's category and has no override.
        assert_eq!(
            settings.style_for(Some("com.microsoft.teams2"), Some("Microsoft Teams")),
            (Style::Casual, StyleSource::Category)
        );
        // No category style for email, so the global style applies.
        assert_eq!(
            settings.style_for(Some("com.apple.mail"), Some("Mail")),
            (Style::Formal, StyleSource::Global)
        );
        assert_eq!(
            settings.style_for(None, None),
            (Style::Formal, StyleSource::Global)
        );
    }

    #[test]
    fn existing_installs_keep_the_global_style_everywhere() {
        let settings = Settings {
            style: Style::Casual,
            ..Settings::default()
        };
        for (bundle, name) in [
            (Some("com.apple.mail"), Some("Mail")),
            (Some("com.microsoft.VSCode"), Some("Code")),
            (None, None),
        ] {
            assert_eq!(
                settings.style_for(bundle, name),
                (Style::Casual, StyleSource::Global)
            );
        }
    }

    #[test]
    fn category_styles_and_app_styles_use_wire_names() {
        let patch: Result<SettingsPatch, _> = serde_json::from_str(
            r#"{"categoryStyles":{"email":"formal","code":"casual"},
                "appStyles":[{"bundleId":"md.obsidian","appName":"Obsidian","style":"casual"}],
                "postProcessing":"local","meetingAssistant":"apple"}"#,
        );
        let patch = patch.ok();
        let categories = patch.as_ref().and_then(|p| p.category_styles.clone());
        assert_eq!(
            categories.and_then(|c| c.get(&AppCategory::Code).copied()),
            Some(Style::Casual)
        );
        assert_eq!(
            patch
                .as_ref()
                .and_then(|p| p.app_styles.as_ref())
                .and_then(|apps| apps.first())
                .map(|app| app.bundle_id.as_str()),
            Some("md.obsidian")
        );
        assert_eq!(
            patch.as_ref().and_then(|p| p.post_processing),
            Some(PostProcessing::Local)
        );
        assert_eq!(
            patch.and_then(|p| p.meeting_assistant),
            Some(LlmBackend::Apple)
        );
    }

    #[test]
    fn context_is_off_by_default_and_never_read_from_excluded_apps() {
        let mut settings = Settings::default();
        assert!(!settings.context_allowed(Some("com.apple.Notes")));
        settings.foreground_context = true;
        assert!(settings.context_allowed(Some("com.apple.Notes")));
        assert!(settings.context_allowed(None));
        assert!(!settings.context_allowed(Some("com.1password.1password")));
        assert!(!settings.context_allowed(Some(" COM.APPLE.PASSWORDS ")));
    }

    #[test]
    fn settings_carry_no_remote_provider_fields() {
        let json = serde_json::to_string(&Settings::default()).unwrap_or_default();
        for removed in ["groq", "customBaseUrl", "customModel", "ApiKey"] {
            assert!(!json.contains(removed), "{removed} still serialized");
        }
    }
}
