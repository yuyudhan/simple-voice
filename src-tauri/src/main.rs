// FilePath: src-tauri/src/main.rs
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    // `simple-voice --version` answers without starting the app, so install scripts and users
    // can check what is installed from a terminal.
    let args: Vec<String> = std::env::args().collect();
    if args
        .iter()
        .skip(1)
        .any(|arg| arg == "--version" || arg == "-V")
    {
        println!("simple-voice {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    // `simple-voice --start` from a key binding or script cannot record without a microphone:
    // say why, in the banner's words, and fail instead of handing the request on in silence.
    if let Some(reason) = simple_voice_lib::start_refusal(&args, || sv_audio::check_input(None)) {
        eprintln!("simple-voice: {reason}");
        return ExitCode::FAILURE;
    }
    match simple_voice_lib::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "Simple Voice failed to start");
            ExitCode::FAILURE
        }
    }
}
