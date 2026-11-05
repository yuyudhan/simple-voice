// FilePath: engine-linux/src/main.rs
//! Entry point: `simple-voice-engine --models-dir <path>`, JSON lines on stdin/stdout, exactly as
//! the Swift helper on macOS (docs/internal/engine.md). Only Linux has an implementation; macOS
//! bundles the Swift helper under the same name.
#![forbid(unsafe_code)]

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    linux::run()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "[error] this is the Linux engine helper; on macOS Simple Voice runs the Swift helper \
         built from engine/"
    );
    std::process::ExitCode::FAILURE
}
