// FilePath: crates/sv-audio/src/devices.rs
//! Input device discovery and the choice of microphone to record from.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};
use sv_domain::{AppResult, CaptureProblem};

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
    Ok(candidates(&host)?
        .into_iter()
        .map(|(_, name)| Microphone {
            id: name.clone(),
            is_default: default_name.as_deref() == Some(name.as_str()),
            is_built_in: is_built_in_name(&name),
            name,
        })
        .collect())
}

/// The input devices to choose from, with their names. ALSA (the Linux host) lists every PCM
/// plugin by description: the null device that records silence, rate converters, each sound card
/// several times. Its `default` PCM is the default source of PulseAudio or PipeWire, which the
/// user picks in the desktop's sound settings, so Linux offers that one only.
fn candidates(host: &cpal::Host) -> Result<Vec<(cpal::Device, String)>, CaptureProblem> {
    if cfg!(target_os = "linux") {
        return Ok(host
            .default_input_device()
            .and_then(|device| device_name(&device).map(|name| (device, name)))
            .into_iter()
            .collect());
    }
    Ok(host
        .input_devices()
        .map_err(|error| capture_problem(&error, None))?
        .filter_map(|device| device_name(&device).map(|name| (device, name)))
        .collect())
}

pub(crate) fn device_name(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_owned())
}

/// Core Audio names the Mac's own microphone after the model ("MacBook Pro Microphone",
/// "iMac Microphone"); older systems and other hosts say "Built-in".
pub(crate) fn is_built_in_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("built-in")
        || lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| word.starts_with("macbook") || word == "imac")
}

/// Resolves the device to record from (see [`choose_input`]) and returns it with its name.
pub(crate) fn select_input_device(
    host: &cpal::Host,
    requested: Option<&str>,
) -> Result<(cpal::Device, String), CaptureProblem> {
    let mut devices = candidates(host)?;
    let names: Vec<&str> = devices.iter().map(|(_, name)| name.as_str()).collect();
    if let Some(wanted) = requested.filter(|wanted| !names.contains(wanted)) {
        tracing::warn!(
            requested = wanted,
            "selected microphone is not connected; using the built-in microphone or default"
        );
    }
    let default_name = host
        .default_input_device()
        .and_then(|device| device_name(&device));
    match choose_input(&names, default_name.as_deref(), requested) {
        Some(index) if index < devices.len() => Ok(devices.swap_remove(index)),
        _ => host
            .default_input_device()
            .and_then(|device| device_name(&device).map(|name| (device, name)))
            .ok_or(CaptureProblem::NoDevice),
    }
}

/// Index into `names` of the device to record from. A connected explicit choice is kept.
/// Otherwise (no choice, or the chosen device went away) the built-in microphone wins over the
/// system default: a Bluetooth headset that becomes the default input drops to the 8 kHz HFP
/// profile the moment it is opened, which ruins both the call audio and the transcription, so
/// connecting one never moves dictation to it.
pub(crate) fn choose_input(
    names: &[&str],
    default_name: Option<&str>,
    requested: Option<&str>,
) -> Option<usize> {
    requested
        .and_then(|wanted| names.iter().position(|name| *name == wanted))
        .or_else(|| built_in_or_default(names, default_name))
}

/// Index into `names` of the first built-in microphone, else the system default (a Mac mini or
/// Mac Studio has no built-in microphone), else the first device.
pub(crate) fn built_in_or_default(names: &[&str], default_name: Option<&str>) -> Option<usize> {
    names
        .iter()
        .position(|name| is_built_in_name(name))
        .or_else(|| default_name.and_then(|wanted| names.iter().position(|name| *name == wanted)))
        .or_else(|| (!names.is_empty()).then_some(0))
}

/// Names a cpal failure on `device` (when one was chosen) as the problem the user can act on.
pub(crate) fn capture_problem(error: &cpal::Error, device: Option<&str>) -> CaptureProblem {
    let named = || device.unwrap_or("The microphone").to_owned();
    match error.kind() {
        cpal::ErrorKind::PermissionDenied => CaptureProblem::PermissionDenied,
        cpal::ErrorKind::DeviceBusy => CaptureProblem::Busy { device: named() },
        cpal::ErrorKind::DeviceNotAvailable => match device {
            Some(device) => CaptureProblem::Disconnected {
                device: device.to_owned(),
            },
            None => CaptureProblem::NoDevice,
        },
        cpal::ErrorKind::UnsupportedConfig => CaptureProblem::Failed {
            device: device.map(str::to_owned),
            reason: format!("it does not support its own default format ({error})"),
        },
        _ => CaptureProblem::Failed {
            device: device.map(str::to_owned),
            reason: error.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_built_in_microphones_by_name() {
        assert!(is_built_in_name("MacBook Pro Microphone"));
        assert!(is_built_in_name("macbook air microphone"));
        assert!(is_built_in_name("Built-in Microphone"));
        assert!(is_built_in_name("iMac Microphone"));
        assert!(!is_built_in_name("AirPods Pro"));
        assert!(!is_built_in_name("Shure MV7"));
        assert!(!is_built_in_name("Primacoustic VoxGuard"));
    }

    #[test]
    fn no_choice_records_from_built_in_even_when_bluetooth_is_the_default() {
        let names = ["WH-XB910N", "MacBook Pro Microphone", "Shure MV7"];
        assert_eq!(choose_input(&names, Some("WH-XB910N"), None), Some(1));
    }

    #[test]
    fn an_explicit_choice_is_kept_while_connected() {
        let names = ["WH-XB910N", "MacBook Pro Microphone"];
        assert_eq!(
            choose_input(&names, Some("MacBook Pro Microphone"), Some("WH-XB910N")),
            Some(0)
        );
    }

    #[test]
    fn a_choice_that_went_away_falls_back_to_built_in() {
        let names = ["Shure MV7", "MacBook Pro Microphone"];
        assert_eq!(
            choose_input(&names, Some("Shure MV7"), Some("WH-XB910N")),
            Some(1)
        );
    }

    #[test]
    fn without_built_in_the_system_default_is_used_then_the_first() {
        let names = ["WH-XB910N", "Shure MV7"];
        assert_eq!(choose_input(&names, Some("Shure MV7"), None), Some(1));
        assert_eq!(
            choose_input(&names, Some("Shure MV7"), Some("Gone")),
            Some(1)
        );
        assert_eq!(choose_input(&names, Some("Gone"), None), Some(0));
        assert_eq!(choose_input(&[], None, Some("WH-XB910N")), None);
    }
}
