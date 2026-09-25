// FilePath: src-tauri/src/main.rs
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match simple_voice_lib::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "Simple Voice failed to start");
            ExitCode::FAILURE
        }
    }
}
