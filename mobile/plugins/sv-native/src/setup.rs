// FilePath: mobile/plugins/sv-native/src/setup.rs
//! Keyboard setup from inside the app: whether the Simple Voice keyboard is added, has what it
//! needs, and buttons that open the phone's own settings pages. Swift (iPhone) and Kotlin
//! (Android) answer; elsewhere every call says setup is a phone feature.

use serde::{Deserialize, Serialize};
use tauri::Runtime;

use crate::NativeBridge;

/// What the setup screen shows. Each step is checked by the phone, never assumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyboardSetup {
    /// The Simple Voice keyboard is added in the phone's keyboard settings.
    pub enabled: bool,
    /// iPhone only: Full Access is on, proven by the keyboard having reached the shared store.
    pub full_access: Option<bool>,
    /// Android only: Simple Voice is the keyboard that opens in text fields.
    pub selected: Option<bool>,
    /// `granted`, `denied` or `undetermined`.
    pub microphone: String,
}

/// The phone's answer to `requestMicrophone`.
#[derive(Debug, Clone, Deserialize)]
struct Microphone {
    microphone: String,
}

const NOT_A_PHONE: &str = "Keyboard setup is only available on a phone";

impl<R: Runtime> NativeBridge<R> {
    /// Asks the phone how far keyboard setup has got. Blocks: call it from a blocking task.
    pub fn keyboard_setup(&self) -> Result<KeyboardSetup, String> {
        self.ask("setupStatus")
    }

    /// Opens the settings page where the keyboard is added (iPhone: the app's own page, with
    /// its Keyboards entry; Android: on-screen keyboards).
    pub fn open_keyboard_settings(&self) -> Result<(), String> {
        self.ask::<serde_json::Value>("openKeyboardSettings")
            .map(|_| ())
    }

    /// Android: shows the system picker to switch to Simple Voice. iPhone has no such picker
    /// for apps; users switch with the globe key.
    pub fn choose_keyboard(&self) -> Result<(), String> {
        self.ask::<serde_json::Value>("chooseKeyboard").map(|_| ())
    }

    /// Shows the microphone prompt, or the app's settings when it was already refused.
    pub fn request_microphone(&self) -> Result<String, String> {
        self.ask::<Microphone>("requestMicrophone")
            .map(|answer| answer.microphone)
    }

    fn ask<T: serde::de::DeserializeOwned>(&self, method: &str) -> Result<T, String> {
        #[cfg(mobile)]
        if let Some(handle) = &self.handle {
            return handle
                .run_mobile_plugin::<T>(method, ())
                .map_err(|error| error.to_string());
        }
        let _ = (&self.handle, method);
        Err(NOT_A_PHONE.to_owned())
    }
}
