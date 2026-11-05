// FilePath: src-tauri/src/main.rs
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // `simple-voice --version` answers without starting the app, so install scripts and users
    // can check what is installed from a terminal.
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("simple-voice {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    match simple_voice_lib::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "Simple Voice failed to start");
            ExitCode::FAILURE
        }
    }
}
