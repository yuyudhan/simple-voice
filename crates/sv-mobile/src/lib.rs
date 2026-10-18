// FilePath: crates/sv-mobile/src/lib.rs
//! The dictation pipeline for the phone apps: Groq Whisper, the deterministic rules, the Groq
//! formatting pass and a history row, with a failed recording's audio kept for Retry; and edit
//! mode, which rewrites a selection by a spoken instruction. The Android keyboard reaches it
//! through sv-ffi; the mobile app's commands call it directly.
#![forbid(unsafe_code)]

mod edit;
mod http;
mod local;
mod pipeline;
mod status;

pub use edit::{edit, Edit, EditRequest};
pub use http::http_client;
pub use local::{dictate_local, discard, LocalTranscript};
pub use pipeline::{dictate, retry, Delivery, Dictation, Recording};
pub use status::{keyboard_status, KeyboardStatus, Reach};
