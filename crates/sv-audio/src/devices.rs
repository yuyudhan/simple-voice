// FilePath: crates/sv-audio/src/devices.rs
//! Input device discovery and the automatic microphone choice.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use sv_domain::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Microphone {
    /// The device name; CoreAudio names are what the user sees and are stable across launches.
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_built_in: bool,
}

/// Every input device the default host can open, in the host's order.
pub fn list_microphones() -> AppResult<Vec<Microphone>> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|device| device_name(&device));
    let devices = host.input_devices().map_err(|error| audio_error(&error))?;
    Ok(devices
        .filter_map(|device| device_name(&device))
        .map(|name| Microphone {
            id: name.clone(),
            is_default: default_name.as_deref() == Some(name.as_str()),
            is_built_in: is_built_in_name(&name),
            name,
        })
        .collect())
}

pub(crate) fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_owned())
}

pub(crate) fn is_built_in_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("macbook") || lower.contains("built-in")
}

/// Resolves the device to record from and returns it with its name.
///
/// Without a request the built-in microphone wins over the system default: a Bluetooth headset
/// that is the default input drops to the 8 kHz HFP profile the moment it is opened, which ruins
/// both the call audio and the transcription. A requested device that has gone away (unplugged
/// headset, renamed interface) falls back to the automatic choice so dictation still works.
pub(crate) fn select_input_device(
    host: &cpal::Host,
    requested: Option<&str>,
) -> AppResult<(cpal::Device, String)> {
    let mut devices: Vec<(cpal::Device, String)> = host
        .input_devices()
        .map_err(|error| audio_error(&error))?
        .filter_map(|device| device_name(&device).map(|name| (device, name)))
        .collect();

    let mut chosen = None;
    if let Some(wanted) = requested {
        chosen = devices.iter().position(|(_, name)| name == wanted);
        if chosen.is_none() {
            tracing::warn!(
                requested = wanted,
                "selected microphone is not connected; using the automatic choice"
            );
        }
    }
    if chosen.is_none() {
        let names: Vec<&str> = devices.iter().map(|(_, name)| name.as_str()).collect();
        let default_name = host
            .default_input_device()
            .and_then(|device| device_name(&device));
        chosen = automatic_choice(&names, default_name.as_deref());
    }
    match chosen {
        Some(index) if index < devices.len() => Ok(devices.swap_remove(index)),
        _ => host
            .default_input_device()
            .and_then(|device| device_name(&device).map(|name| (device, name)))
            .ok_or_else(|| AppError::Audio("No microphone is connected.".to_owned())),
    }
}

/// Index into `names` of the device to use: first built-in, else the default, else the first.
pub(crate) fn automatic_choice(names: &[&str], default_name: Option<&str>) -> Option<usize> {
    names
        .iter()
        .position(|name| is_built_in_name(name))
        .or_else(|| default_name.and_then(|wanted| names.iter().position(|name| *name == wanted)))
        .or_else(|| (!names.is_empty()).then_some(0))
}

/// Turns a cpal failure into a message that tells the user what to do about it.
pub(crate) fn audio_error(error: &cpal::Error) -> AppError {
    let message = match error.kind() {
        cpal::ErrorKind::PermissionDenied => "Microphone access is denied. Allow Simple Voice in \
             System Settings → Privacy & Security → Microphone, then try again."
            .to_owned(),
        cpal::ErrorKind::DeviceBusy => {
            "The microphone is busy in another app. Close that app or pick another microphone."
                .to_owned()
        }
        cpal::ErrorKind::DeviceNotAvailable => {
            "The microphone was disconnected or is not available.".to_owned()
        }
        cpal::ErrorKind::UnsupportedConfig => {
            format!("The microphone does not support its own default format: {error}")
        }
        _ => format!("Microphone error: {error}"),
    };
    AppError::Audio(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_built_in_microphones_by_name() {
        assert!(is_built_in_name("MacBook Pro Microphone"));
        assert!(is_built_in_name("macbook air microphone"));
        assert!(is_built_in_name("Built-in Microphone"));
        assert!(!is_built_in_name("AirPods Pro"));
        assert!(!is_built_in_name("Shure MV7"));
    }

    #[test]
    fn automatic_choice_prefers_built_in_over_default() {
        let names = ["AirPods Pro", "MacBook Pro Microphone", "Shure MV7"];
        assert_eq!(automatic_choice(&names, Some("AirPods Pro")), Some(1));
    }

    #[test]
    fn automatic_choice_falls_back_to_default_then_first() {
        let names = ["AirPods Pro", "Shure MV7"];
        assert_eq!(automatic_choice(&names, Some("Shure MV7")), Some(1));
        assert_eq!(automatic_choice(&names, Some("Gone")), Some(0));
        assert_eq!(automatic_choice(&[], None), None);
    }
}
