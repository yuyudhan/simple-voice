// FilePath: crates/sv-domain/src/health/input.rs
//! What the app knows about the microphone: whether one is there to record from (polled while
//! nothing records) and what went wrong with the last attempt to record.

use serde::{Deserialize, Serialize};

/// The outcome of the last check for a microphone to record from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum InputState {
    /// Nothing checked yet; no claim either way.
    #[default]
    Unknown,
    /// Dictation records from `device`.
    Ready { device: String },
    /// The microphone chosen in Settings is gone, so dictation records from `device` instead.
    FellBack { requested: String, device: String },
    /// No microphone at all (a Mac mini with only HDMI audio, a PC whose only source is the
    /// speaker monitor).
    NoDevice,
    /// Linux: neither PipeWire nor PulseAudio runs for this session, so nothing can record.
    NoSoundServer,
}

/// Why a recording could not start, ended early, or captured nothing usable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "problem",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CaptureProblem {
    /// There was no microphone to open.
    NoDevice,
    /// The operating system refused access to the microphone.
    PermissionDenied,
    /// Another app holds the microphone exclusively.
    Busy { device: String },
    /// The microphone went away while recording (unplugged, Bluetooth dropped, stream died).
    Disconnected { device: String },
    /// The stream opened but delivered no audio at all.
    NoAudio { device: String },
    /// The stream delivers digital silence: a hardware mute switch or an input volume of zero.
    Silent { device: String },
    /// Opening or running the stream failed for another reason, in the audio system's words.
    Failed {
        device: Option<String>,
        reason: String,
    },
}

impl CaptureProblem {
    /// Whether the recording cannot go on: everything except silence, which may still end
    /// when the user unmutes.
    pub fn ends_recording(&self) -> bool {
        !matches!(self, Self::Silent { .. })
    }
}
