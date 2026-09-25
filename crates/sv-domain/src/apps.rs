// FilePath: crates/sv-domain/src/apps.rs
//! Groups the app a dictation was pasted into, for Insights → usage by kind of app.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    WorkMessages,
    PersonalMessages,
    Email,
    Documents,
    AiPrompts,
    Code,
    Other,
}

impl AppCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkMessages => "work_messages",
            Self::PersonalMessages => "personal_messages",
            Self::Email => "email",
            Self::Documents => "documents",
            Self::AiPrompts => "ai_prompts",
            Self::Code => "code",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "work_messages" => Self::WorkMessages,
            "personal_messages" => Self::PersonalMessages,
            "email" => Self::Email,
            "documents" => Self::Documents,
            "ai_prompts" => Self::AiPrompts,
            "code" => Self::Code,
            _ => Self::Other,
        }
    }
}

/// Bundle-id prefixes first (exact and stable), then app-name keywords for browsers and apps
/// without a well-known id. Browsers stay `Other`: the tab is unknown.
const BY_BUNDLE: &[(&str, AppCategory)] = &[
    ("com.tinyspeck.slackmacgap", AppCategory::WorkMessages),
    ("com.microsoft.teams", AppCategory::WorkMessages),
    ("com.hnc.Discord", AppCategory::PersonalMessages),
    ("net.whatsapp.WhatsApp", AppCategory::PersonalMessages),
    ("desktop.WhatsApp", AppCategory::PersonalMessages),
    ("ru.keepcoder.Telegram", AppCategory::PersonalMessages),
    ("org.telegram.desktop", AppCategory::PersonalMessages),
    ("com.apple.MobileSMS", AppCategory::PersonalMessages),
    (
        "org.whispersystems.signal-desktop",
        AppCategory::PersonalMessages,
    ),
    ("com.apple.mail", AppCategory::Email),
    ("com.microsoft.Outlook", AppCategory::Email),
    ("com.superhuman.electron", AppCategory::Email),
    ("com.readdle.smartemail-Mac", AppCategory::Email),
    ("com.apple.Notes", AppCategory::Documents),
    ("com.apple.iWork.Pages", AppCategory::Documents),
    ("com.microsoft.Word", AppCategory::Documents),
    ("notion.id", AppCategory::Documents),
    ("md.obsidian", AppCategory::Documents),
    ("com.openai.chat", AppCategory::AiPrompts),
    ("com.anthropic.claudefordesktop", AppCategory::AiPrompts),
    ("com.todesktop.230313mzl4w4u92", AppCategory::Code),
    ("com.microsoft.VSCode", AppCategory::Code),
    ("dev.zed.Zed", AppCategory::Code),
    ("com.apple.dt.Xcode", AppCategory::Code),
    ("com.jetbrains.", AppCategory::Code),
    ("com.github.wez.wezterm", AppCategory::Code),
    ("com.googlecode.iterm2", AppCategory::Code),
    ("com.apple.Terminal", AppCategory::Code),
    ("com.mitchellh.ghostty", AppCategory::Code),
    ("dev.warp.Warp", AppCategory::Code),
    ("org.gnu.Emacs", AppCategory::Code),
];

const BY_NAME: &[(&str, AppCategory)] = &[
    ("slack", AppCategory::WorkMessages),
    ("teams", AppCategory::WorkMessages),
    ("whatsapp", AppCategory::PersonalMessages),
    ("telegram", AppCategory::PersonalMessages),
    ("messages", AppCategory::PersonalMessages),
    ("discord", AppCategory::PersonalMessages),
    ("mail", AppCategory::Email),
    ("outlook", AppCategory::Email),
    ("chatgpt", AppCategory::AiPrompts),
    ("claude", AppCategory::AiPrompts),
    ("perplexity", AppCategory::AiPrompts),
    ("cursor", AppCategory::Code),
    ("code", AppCategory::Code),
    ("terminal", AppCategory::Code),
    ("notes", AppCategory::Documents),
    ("docs", AppCategory::Documents),
    ("word", AppCategory::Documents),
];

pub fn categorize(bundle_id: Option<&str>, app_name: Option<&str>) -> AppCategory {
    if let Some(id) = bundle_id {
        if let Some((_, category)) = BY_BUNDLE.iter().find(|(prefix, _)| id.starts_with(prefix)) {
            return *category;
        }
    }
    if let Some(name) = app_name {
        let name = name.to_lowercase();
        if let Some((_, category)) = BY_NAME.iter().find(|(word, _)| name.contains(word)) {
            return *category;
        }
    }
    AppCategory::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_id_wins_over_name() {
        assert_eq!(
            categorize(Some("com.tinyspeck.slackmacgap"), Some("Mail")),
            AppCategory::WorkMessages
        );
    }

    #[test]
    fn jetbrains_prefix_covers_every_ide() {
        assert_eq!(
            categorize(Some("com.jetbrains.rustrover"), None),
            AppCategory::Code
        );
    }

    #[test]
    fn unknown_app_is_other() {
        assert_eq!(
            categorize(Some("com.google.Chrome"), Some("Google Chrome")),
            AppCategory::Other
        );
        assert_eq!(categorize(None, None), AppCategory::Other);
    }
}
