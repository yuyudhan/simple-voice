// FilePath: crates/sv-domain/src/permissions.rs
//! macOS privacy permissions the app needs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    /// Recording audio. Required for every dictation.
    Microphone,
    /// Posting Cmd+V into other apps. Required for pasting.
    Accessibility,
    /// Speech Recognition. Required only for the Apple Speech model.
    Speech,
}

impl PermissionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Accessibility => "accessibility",
            Self::Speech => "speech",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionStatus {
    Granted,
    Denied,
    NotDetermined,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub microphone: PermissionStatus,
    pub accessibility: PermissionStatus,
    pub speech: PermissionStatus,
}
