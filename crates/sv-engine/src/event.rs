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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum EngineEvent {
    FnKey { action: FnKeyAction },
}

/// Receives every event the helper emits; called on the thread that forwards stdout.
pub type EventHandler = Box<dyn Fn(EngineEvent) + Send + Sync>;
