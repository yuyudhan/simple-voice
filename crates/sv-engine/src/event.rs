// FilePath: crates/sv-engine/src/event.rs
//! Unsolicited lines the helper writes without a request id (architecture § 6).

use serde::Deserialize;

/// What the Fn (Globe) key did. The helper already filters out Fn used as a modifier
/// (Fn+Arrow, Fn+Delete), so every action here is the user reaching for the Fn key alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FnKeyAction {
    /// Fn has been held long enough, with nothing else pressed, to count as a hold.
    Press,
    /// Fn went up after a `Press`.
    Release,
    /// Fn went down and up before it counted as a hold.
    Tap,
    /// Another key or modifier joined Fn after a `Press`; the press belonged to a combination.
    Chord,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    FnKey {
        action: FnKeyAction,
    },
    /// The live hypothesis of a stream changed. `stable` is the committed prefix of `text`;
    /// the rest of `text` may still be revised.
    StreamPartial {
        #[serde(rename = "streamId")]
        stream_id: u64,
        text: String,
        stable: String,
    },
    /// The stream is dead (recogniser error, more than 30 s of audio queued, or no audio within
    /// 3 s of its start); transcribe the recording instead.
    StreamFailed {
        #[serde(rename = "streamId")]
        stream_id: u64,
        reason: String,
        /// Set when the input is to blame (no audio, no device, no permission).
        #[serde(default)]
        problem: Option<StreamProblem>,
    },
    /// The system-audio capture stopped by itself; its WAV keeps what was written.
    SystemAudioFailed {
        reason: String,
    },
    /// An input device was plugged in or removed, or the default input changed (Core Audio's
    /// device-change notification on macOS, `pactl subscribe` on Linux).
    InputDevicesChanged,
}

/// Why a live stream got no usable audio, in the helper's protocol words: `problem` is
/// `no_audio`, `no_device` or `permission_denied`, the same names as the app's capture problems.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StreamProblem {
    pub problem: String,
    #[serde(default)]
    pub device: Option<String>,
}

/// Receives every event the helper emits; called on the thread that forwards stdout.
pub type EventHandler = Box<dyn Fn(EngineEvent) + Send + Sync>;
