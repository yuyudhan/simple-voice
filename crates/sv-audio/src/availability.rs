// FilePath: crates/sv-audio/src/availability.rs
//! Whether there is a microphone to record from, checked without opening a stream so it can
//! run every few seconds while nothing records. The app polls it to show "No microphone
//! connected" before anyone presses the shortcut, and to clear that the moment one appears.
//!
//! macOS asks Core Audio for its input devices. Linux records from the sound server's default
//! source through ALSA's `default` PCM, which "works" even when that source is only the
//! speaker monitor (a PC with no microphone records its own output); so on Linux the check asks
//! the server itself with `pactl`, counts only real sources, and first looks for the server's
//! socket so a session without PipeWire or PulseAudio is named as such.

use std::path::Path;
use std::process::Command;

use cpal::traits::HostTrait;
use sv_domain::{CaptureProblem, InputState};

use crate::devices::{automatic_choice, capture_problem, device_name};

/// What the audio system offers right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputProbe {
    /// PipeWire or PulseAudio runs for this session (always true on macOS).
    pub sound_server: bool,
    /// Inputs that record a microphone, speaker monitors excluded.
    pub devices: Vec<String>,
    /// The system's default input, when it is one of `devices`.
    pub default: Option<String>,
}

/// The microphone a dictation would record from, given what the system offers and the one
/// chosen in Settings.
pub fn input_state(probe: &InputProbe, requested: Option<&str>) -> InputState {
    if !probe.sound_server {
        return InputState::NoSoundServer;
    }
    let names: Vec<&str> = probe.devices.iter().map(String::as_str).collect();
    let Some(automatic) = automatic_choice(&names, probe.default.as_deref()) else {
        return InputState::NoDevice;
    };
    let fallback = names.get(automatic).copied().unwrap_or_default().to_owned();
    match requested {
        Some(wanted) if names.contains(&wanted) => InputState::Ready {
            device: wanted.to_owned(),
        },
        Some(wanted) => InputState::FellBack {
            requested: wanted.to_owned(),
            device: fallback,
        },
        None => InputState::Ready { device: fallback },
    }
}

/// Checks the system's inputs now. A check that itself fails claims nothing (`Unknown`): the
/// next recording reports what is wrong.
pub fn check_input(requested: Option<&str>) -> InputState {
    let probe = if cfg!(target_os = "linux") {
        probe_sound_server()
    } else {
        probe_host()
    };
    match probe {
        // Linux records the server's default source; a choice by name does not apply there.
        Ok(probe) if cfg!(target_os = "linux") => input_state(&probe, None),
        Ok(probe) => input_state(&probe, requested),
        Err(problem) => {
            tracing::debug!(?problem, "input check failed");
            InputState::Unknown
        }
    }
}

/// The input devices the audio host lists (Core Audio on macOS).
fn probe_host() -> Result<InputProbe, CaptureProblem> {
    let host = cpal::default_host();
    let default = host
        .default_input_device()
        .and_then(|device| device_name(&device));
    let devices: Vec<String> = host
        .input_devices()
        .map_err(|error| capture_problem(&error, None))?
        .filter_map(|device| device_name(&device))
        .collect();
    Ok(InputProbe {
        sound_server: true,
        default: default.filter(|name| devices.contains(name)),
        devices,
    })
}

/// Linux: the session's sound server and its sources.
fn probe_sound_server() -> Result<InputProbe, CaptureProblem> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR");
    if let Some(dir) = runtime.as_deref().map(Path::new) {
        let running = dir.join("pulse/native").exists() || dir.join("pipewire-0").exists();
        if !running {
            return Ok(InputProbe {
                sound_server: false,
                devices: Vec::new(),
                default: None,
            });
        }
    }
    let sources = pactl(&["list", "sources"]);
    let info = pactl(&["info"]);
    match (sources, info) {
        (Some(sources), Some(info)) => {
            let sources = parse_sources(&sources);
            let default_name = parse_default_source(&info);
            Ok(sources_probe(&sources, default_name.as_deref()))
        }
        // No pactl (a bare PipeWire without pipewire-pulse tools): ask ALSA's default PCM.
        _ => probe_alsa_default(),
    }
}

fn probe_alsa_default() -> Result<InputProbe, CaptureProblem> {
    let devices: Vec<String> = cpal::default_host()
        .default_input_device()
        .and_then(|device| device_name(&device))
        .into_iter()
        .collect();
    Ok(InputProbe {
        sound_server: true,
        default: devices.first().cloned(),
        devices,
    })
}

/// Runs `pactl` with untranslated output; `None` when it is missing or fails.
fn pactl(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl")
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// One source of `pactl list sources`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    name: String,
    description: String,
    monitor: bool,
}

fn parse_sources(text: &str) -> Vec<Source> {
    let mut sources: Vec<Source> = Vec::new();
    for line in text.lines() {
        if line.starts_with("Source #") {
            sources.push(Source {
                name: String::new(),
                description: String::new(),
                monitor: false,
            });
            continue;
        }
        let Some(source) = sources.last_mut() else {
            continue;
        };
        // Only the block's own fields, not the indented property list below them.
        let Some(field) = line
            .strip_prefix('\t')
            .filter(|rest| !rest.starts_with('\t'))
        else {
            continue;
        };
        if let Some(name) = field.strip_prefix("Name: ") {
            source.name = name.trim().to_owned();
            source.monitor |= source.name.ends_with(".monitor");
        } else if let Some(description) = field.strip_prefix("Description: ") {
            source.description = description.trim().to_owned();
        } else if let Some(sink) = field.strip_prefix("Monitor of Sink: ") {
            source.monitor |= sink.trim() != "n/a";
        }
    }
    sources
}

fn parse_default_source(info: &str) -> Option<String> {
    info.lines()
        .find_map(|line| line.strip_prefix("Default Source: "))
        .map(|name| name.trim().to_owned())
}

fn sources_probe(sources: &[Source], default_name: Option<&str>) -> InputProbe {
    let label = |source: &Source| {
        if source.description.is_empty() {
            source.name.clone()
        } else {
            source.description.clone()
        }
    };
    let microphones: Vec<&Source> = sources.iter().filter(|source| !source.monitor).collect();
    InputProbe {
        sound_server: true,
        devices: microphones.iter().map(|source| label(source)).collect(),
        default: microphones
            .iter()
            .find(|source| Some(source.name.as_str()) == default_name)
            .map(|source| label(source)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `pactl list sources` on a PC whose only source is the speaker monitor (itlinux1).
    const MONITOR_ONLY: &str = "Source #0\n\
        \tState: IDLE\n\
        \tName: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n\
        \tDescription: Monitor of Built-in Audio Analog Stereo\n\
        \tMonitor of Sink: alsa_output.pci-0000_00_1f.3.analog-stereo\n\
        \tProperties:\n\
        \t\tdevice.description = \"Monitor of Built-in Audio Analog Stereo\"\n";

    const WITH_HEADSET: &str = "Source #0\n\
        \tName: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n\
        \tDescription: Monitor of Built-in Audio Analog Stereo\n\
        \tMonitor of Sink: alsa_output.pci-0000_00_1f.3.analog-stereo\n\
        Source #1\n\
        \tName: alsa_input.usb-Jabra_Evolve_20-00.mono-fallback\n\
        \tDescription: Jabra EVOLVE 20 Mono\n\
        \tMonitor of Sink: n/a\n\
        \tProperties:\n\
        \t\tdevice.description = \"Jabra EVOLVE 20\"\n";

    const INFO: &str = "Server Name: pulseaudio\n\
        Default Sink: alsa_output.pci-0000_00_1f.3.analog-stereo\n\
        Default Source: alsa_output.pci-0000_00_1f.3.analog-stereo.monitor\n";

    fn probe(devices: &[&str], default: Option<&str>) -> InputProbe {
        InputProbe {
            sound_server: true,
            devices: devices.iter().map(|name| (*name).to_owned()).collect(),
            default: default.map(str::to_owned),
        }
    }

    #[test]
    fn no_input_device_is_no_device() {
        assert_eq!(input_state(&probe(&[], None), None), InputState::NoDevice);
        assert_eq!(
            input_state(&probe(&[], None), Some("AirPods Pro")),
            InputState::NoDevice
        );
    }

    #[test]
    fn the_automatic_choice_prefers_the_built_in_microphone() {
        let state = input_state(
            &probe(
                &["AirPods Pro", "MacBook Pro Microphone"],
                Some("AirPods Pro"),
            ),
            None,
        );
        assert_eq!(
            state,
            InputState::Ready {
                device: "MacBook Pro Microphone".to_owned()
            }
        );
    }

    #[test]
    fn a_selected_microphone_that_went_away_falls_back() {
        let devices = probe(&["MacBook Pro Microphone"], Some("MacBook Pro Microphone"));
        assert_eq!(
            input_state(&devices, Some("Shure MV7")),
            InputState::FellBack {
                requested: "Shure MV7".to_owned(),
                device: "MacBook Pro Microphone".to_owned()
            }
        );
        let plugged = probe(&["MacBook Pro Microphone", "Shure MV7"], None);
        assert_eq!(
            input_state(&plugged, Some("Shure MV7")),
            InputState::Ready {
                device: "Shure MV7".to_owned()
            }
        );
    }

    #[test]
    fn no_sound_server_is_named() {
        let probe = InputProbe {
            sound_server: false,
            devices: Vec::new(),
            default: None,
        };
        assert_eq!(input_state(&probe, None), InputState::NoSoundServer);
    }

    #[test]
    fn a_speaker_monitor_is_not_a_microphone() {
        let sources = parse_sources(MONITOR_ONLY);
        assert_eq!(sources.len(), 1);
        assert!(sources[0].monitor);
        let probe = sources_probe(&sources, parse_default_source(INFO).as_deref());
        assert_eq!(probe.devices, Vec::<String>::new());
        assert_eq!(input_state(&probe, None), InputState::NoDevice);
    }

    #[test]
    fn a_real_source_is_found_by_its_description() {
        let sources = parse_sources(WITH_HEADSET);
        assert_eq!(sources.len(), 2);
        assert!(!sources[1].monitor);
        assert_eq!(sources[1].description, "Jabra EVOLVE 20 Mono");
        let probe = sources_probe(
            &sources,
            Some("alsa_input.usb-Jabra_Evolve_20-00.mono-fallback"),
        );
        assert_eq!(probe.default.as_deref(), Some("Jabra EVOLVE 20 Mono"));
        assert_eq!(
            input_state(&probe, None),
            InputState::Ready {
                device: "Jabra EVOLVE 20 Mono".to_owned()
            }
        );
    }

    #[test]
    fn the_default_source_comes_from_pactl_info() {
        assert_eq!(
            parse_default_source(INFO).as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor")
        );
        assert_eq!(parse_default_source("Server Name: pulseaudio\n"), None);
    }
}
