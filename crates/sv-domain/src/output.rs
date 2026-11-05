// FilePath: crates/sv-domain/src/output.rs
//! Muting system output while a dictation records.

use serde::{Deserialize, Serialize};

/// The output device a recording muted, with what unmuting exactly it needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MutedOutput {
    /// The platform's stable device identifier (Core Audio device UID, PulseAudio sink name), so
    /// the same device is unmuted even when the default output changed meanwhile.
    pub device: String,
    /// Set when the device has no mute control and was silenced by turning its volume to zero:
    /// the volume (0.0–1.0) to put back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f32>,
}
