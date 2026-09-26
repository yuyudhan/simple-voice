// FilePath: crates/sv-engine/src/lib.rs
//! Client for the Swift engine helper (`simple-voice-engine`).
//!
//! The app spawns the sidecar and wires its pipes; this crate only speaks the line-delimited
//! JSON protocol, so it has no Tauri dependency and is tested with an in-memory transport.
#![forbid(unsafe_code)]

mod client;
mod event;
mod protocol;

pub use client::{EngineClient, ProgressCallback, Transport};
pub use event::{EngineEvent, EventHandler, FnKeyAction};
pub use protocol::{
    EditWatch, EngineTranscript, FrontmostApp, LoginItemStatus, ModelStatusResult, PingResult,
    PolishReply,
};
