// FilePath: crates/sv-ffi/src/lib.rs
//! The Rust core for the Android keyboard. Android runs the keyboard as its own service, often
//! while the app's screen (and with it Tauri) is closed, so the keyboard cannot use Tauri
//! commands. These JNI functions, linked into the same library as the Tauri app, are its way in.
//!
//! This crate may not `#![forbid(unsafe_code)]`: exporting a JNI name needs `#[no_mangle]`.
//! It contains no `unsafe` block, function or impl.

mod core;
pub mod keyboard;
