// FilePath: crates/sv-mobile/src/lib.rs
//! The dictation pipeline for the phone apps: Groq Whisper, the deterministic rules, the Groq
//! formatting pass and a history row, with a failed recording's audio kept for Retry. The
//! Android keyboard reaches it through sv-ffi; the mobile app's commands call it directly.
#![forbid(unsafe_code)]

mod http;
mod pipeline;
mod status;

pub use http::http_client;
pub use pipeline::{dictate, retry, Delivery, Dictation, Recording};
pub use status::{keyboard_status, KeyboardStatus};
