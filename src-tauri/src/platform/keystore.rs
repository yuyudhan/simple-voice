// FilePath: src-tauri/src/platform/keystore.rs
//! Secrets the user hands the app (the Groq API key), in the system's secret store: a generic
//! password in the login Keychain on macOS; on Linux an item in the Secret Service keyring
//! (GNOME Keyring, KWallet) through `secret-tool` from libsecret. Every call blocks and may show
//! the system's unlock prompt, so callers run them off the async workers. Errors are messages
//! for the user.

/// What the user knows the store as.
#[cfg(target_os = "macos")]
pub(crate) const STORE_NAME: &str = "Keychain";
#[cfg(target_os = "linux")]
pub(crate) const STORE_NAME: &str = "keyring";

/// `errSecItemNotFound`: nothing saved under the name.
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25_300;

#[cfg(target_os = "macos")]
pub(crate) fn read(service: &str, account: &str) -> Result<Option<String>, String> {
    match security_framework::passwords::get_generic_password(service, account) {
        Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn write(service: &str, account: &str, secret: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(service, account, secret.as_bytes())
        .map_err(|error| error.to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn delete(service: &str, account: &str) -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(service, account) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::{delete, read, write};

#[cfg(target_os = "linux")]
mod linux {
    use std::io::Write;
    use std::process::{Command, Output, Stdio};

    const SECRET_TOOL: &str = "secret-tool";
    const LABEL: &str = "Simple Voice: Groq API key";

    /// `secret-tool lookup` prints the secret, or exits 1 silently when there is none.
    pub(crate) fn read(service: &str, account: &str) -> Result<Option<String>, String> {
        let output = run(&["lookup", "service", service, "account", account], None)?;
        if output.status.success() {
            return Ok(String::from_utf8(output.stdout).ok());
        }
        match stderr(&output) {
            None => Ok(None),
            Some(error) => Err(error),
        }
    }

    /// The secret goes in on stdin, never on the command line where `ps` would show it.
    pub(crate) fn write(service: &str, account: &str, secret: &str) -> Result<(), String> {
        let args = [
            "store", "--label", LABEL, "service", service, "account", account,
        ];
        let output = run(&args, Some(secret))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(stderr(&output)
                .unwrap_or_else(|| format!("secret-tool exited with {}", output.status)))
        }
    }

    /// Clearing an item that does not exist is not an error.
    pub(crate) fn delete(service: &str, account: &str) -> Result<(), String> {
        let output = run(&["clear", "service", service, "account", account], None)?;
        match stderr(&output) {
            Some(error) if !output.status.success() => Err(error),
            _ => Ok(()),
        }
    }

    fn run(args: &[&str], input: Option<&str>) -> Result<Output, String> {
        let mut child = Command::new(SECRET_TOOL)
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => {
                    "secret-tool is missing; install libsecret-tools".to_owned()
                }
                _ => format!("could not run secret-tool: {error}"),
            })?;
        if let (Some(secret), Some(mut stdin)) = (input, child.stdin.take()) {
            stdin
                .write_all(secret.as_bytes())
                .map_err(|error| format!("could not pass the key to secret-tool: {error}"))?;
        }
        child
            .wait_with_output()
            .map_err(|error| format!("secret-tool stopped: {error}"))
    }

    fn stderr(output: &Output) -> Option<String> {
        let text = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        (!text.is_empty()).then_some(text)
    }
}
