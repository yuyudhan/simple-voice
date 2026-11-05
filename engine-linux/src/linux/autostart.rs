// FilePath: engine-linux/src/linux/autostart.rs
//! Launch at login on Linux: an XDG autostart entry (`$XDG_CONFIG_HOME/autostart`), which GNOME,
//! KDE and the other desktops start at login. It runs the app beside this helper with
//! `--autostart`, which is how the app tells a login launch from a manual one. A desktop that
//! switched the entry off (GNOME's `X-GNOME-Autostart-enabled=false`) reports it disabled.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Debug builds (`just dev`) keep an entry of their own, apart from the installed app's.
const ENTRY: &str = if cfg!(debug_assertions) {
    "dev.yuyudhan.simplevoice.dev.desktop"
} else {
    "dev.yuyudhan.simplevoice.desktop"
};
const APP_BINARY: &str = "simple-voice";

pub(crate) fn status() -> Result<Value, String> {
    Ok(json!({ "status": status_of(&entry_path()?) }))
}

pub(crate) fn set(enabled: bool) -> Result<Value, String> {
    let path = entry_path()?;
    if enabled {
        let app = app_binary()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|error| format!("cannot create {}: {error}", dir.display()))?;
        }
        std::fs::write(&path, entry(&app))
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    } else {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot remove {}: {error}", path.display())),
        }
    }
    status()
}

fn status_of(path: &Path) -> &'static str {
    match std::fs::read_to_string(path) {
        Ok(text) if is_switched_off(&text) => "disabled",
        Ok(_) => "enabled",
        Err(_) => "disabled",
    }
}

fn is_switched_off(text: &str) -> bool {
    text.lines().map(str::trim).any(|line| {
        line.eq_ignore_ascii_case("Hidden=true")
            || line.eq_ignore_ascii_case("X-GNOME-Autostart-enabled=false")
    })
}

fn entry_path() -> Result<PathBuf, String> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .ok_or("neither XDG_CONFIG_HOME nor HOME is set")?;
    Ok(config.join("autostart").join(ENTRY))
}

/// The app installs this helper next to its own executable.
fn app_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let app = exe
        .parent()
        .map(|dir| dir.join(APP_BINARY))
        .filter(|app| app.is_file())
        .ok_or_else(|| format!("Simple Voice is not installed next to {}", exe.display()))?;
    Ok(app)
}

fn entry(app: &Path) -> String {
    let exec = app
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "[Desktop Entry]\nType=Application\nName=Simple Voice\nComment=Private dictation\n\
         Exec=\"{exec}\" --autostart\nIcon=simple-voice\nTerminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_an_entry_that_launches_the_app_at_login() {
        let text = entry(Path::new("/home/a b/.local/bin/simple-voice"));
        assert!(text.contains("Exec=\"/home/a b/.local/bin/simple-voice\" --autostart\n"));
        assert!(!is_switched_off(&text));
    }

    #[test]
    fn reads_an_entry_the_desktop_switched_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(ENTRY);
        assert_eq!(status_of(&path), "disabled");
        std::fs::write(&path, entry(Path::new("/usr/bin/simple-voice"))).unwrap();
        assert_eq!(status_of(&path), "enabled");
        let off = entry(Path::new("/usr/bin/simple-voice")).replace(
            "X-GNOME-Autostart-enabled=true",
            "X-GNOME-Autostart-enabled=false",
        );
        std::fs::write(&path, off).unwrap();
        assert_eq!(status_of(&path), "disabled");
    }
}
