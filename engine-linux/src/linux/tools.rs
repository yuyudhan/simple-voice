// FilePath: engine-linux/src/linux/tools.rs
//! The desktop tools the helper drives (xdotool, ydotool, xclip, wl-paste, playerctl, pactl):
//! looked up on `PATH`, run without a shell and with a timeout, so a wedged tool never wedges a
//! request.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

const TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

/// Runs `program` with `args` and waits for it, at most [`TIMEOUT`].
pub(crate) async fn run(program: &str, args: &[&str]) -> Result<Ran, String> {
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(TIMEOUT, child).await {
        Ok(Ok(output)) => Ok(Ran {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        }),
        Ok(Err(error)) => Err(format!("could not run {program}: {error}")),
        Err(_) => Err(format!(
            "{program} did not finish within {} s",
            TIMEOUT.as_secs()
        )),
    }
}

/// Runs `program` and fails with its stderr unless it exits successfully.
pub(crate) async fn run_ok(program: &str, args: &[&str]) -> Result<String, String> {
    let ran = run(program, args).await?;
    if ran.success {
        Ok(ran.stdout)
    } else if ran.stderr.is_empty() {
        Err(format!("{program} failed"))
    } else {
        Err(format!("{program} failed: {}", ran.stderr))
    }
}

/// The executable `program` resolves to on `PATH`, if any.
pub(crate) fn find(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

pub(crate) fn installed(program: &str) -> bool {
    find(program).is_some()
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_a_program_and_reports_failure() {
        let ran = run("sh", &["-c", "echo out; echo err >&2; exit 3"])
            .await
            .unwrap();
        assert!(!ran.success);
        assert_eq!(ran.stdout, "out\n");
        assert_eq!(ran.stderr, "err");
        assert_eq!(run_ok("true", &[]).await.unwrap(), "");
        assert!(run_ok("false", &[])
            .await
            .unwrap_err()
            .contains("false failed"));
        assert!(run("simple-voice-no-such-tool", &[]).await.is_err());
    }

    #[test]
    fn finds_programs_on_the_path() {
        assert!(installed("sh"));
        assert!(!installed("simple-voice-no-such-tool"));
    }
}
