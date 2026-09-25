// FilePath: crates/sv-storage/src/paths.rs
//! Filesystem layout under `~/.simplevoice/`.
//!
//! The public functions resolve the real data directory. The `*_in` variants take the data
//! directory explicitly so tests (and `Db::open_at`) never touch the user's home.

use std::fs::{self, DirBuilder, OpenOptions, Permissions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sv_domain::{AppError, AppResult};

pub(crate) const DATABASE_FILE: &str = "simple-voice.db";
const DATA_DIR_NAME: &str = ".simplevoice";
pub(crate) const LOCATION_FILE: &str = "location";
const MODELS_DIR: &str = "models";
const AUDIO_DIR: &str = "audio";
const BACKUPS_DIR: &str = "backups";

/// `~/.simplevoice`, created with mode 0700.
pub fn data_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::Io("Could not find your home directory".to_owned()))?;
    let dir = home.join(DATA_DIR_NAME);
    ensure_private_dir(&dir)?;
    Ok(dir)
}

/// `data_dir/models`, owned by the engine helper.
pub fn models_dir() -> AppResult<PathBuf> {
    private_subdir(&data_dir()?, MODELS_DIR)
}

/// `data_dir/audio`, WAVs of failed dictations kept for retry.
pub fn audio_dir() -> AppResult<PathBuf> {
    private_subdir(&data_dir()?, AUDIO_DIR)
}

/// `data_dir/backups`, database copies taken before migrations.
pub fn backups_dir() -> AppResult<PathBuf> {
    backups_dir_in(&data_dir()?)
}

/// Directory holding the database: the path in `data_dir/location`, else `data_dir`.
pub fn database_dir() -> AppResult<PathBuf> {
    database_dir_in(&data_dir()?)
}

pub(crate) fn backups_dir_in(data_dir: &Path) -> AppResult<PathBuf> {
    private_subdir(data_dir, BACKUPS_DIR)
}

pub(crate) fn database_dir_in(data_dir: &Path) -> AppResult<PathBuf> {
    match fs::read_to_string(data_dir.join(LOCATION_FILE)) {
        Ok(contents) => {
            let trimmed = contents.trim();
            if trimmed.is_empty() {
                Ok(data_dir.to_path_buf())
            } else {
                Ok(PathBuf::from(trimmed))
            }
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(data_dir.to_path_buf()),
        Err(error) => Err(AppError::io(error)),
    }
}

/// Records where the database lives. `None` (or the data directory itself) removes the pointer
/// so the default location applies again.
pub(crate) fn write_location(data_dir: &Path, database_dir: Option<&Path>) -> AppResult<()> {
    let pointer = data_dir.join(LOCATION_FILE);
    let Some(database_dir) = database_dir.filter(|dir| !same_dir(dir, data_dir)) else {
        return remove_if_exists(&pointer);
    };
    let text = database_dir.to_str().ok_or_else(|| {
        AppError::InvalidInput("The database folder path must be valid UTF-8".to_owned())
    })?;
    // Write-then-rename so a crash never leaves a half-written pointer that would send the next
    // launch to a wrong (empty) database.
    let staging = data_dir.join(format!("{LOCATION_FILE}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&staging)?;
    file.write_all(text.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&staging, &pointer)?;
    Ok(())
}

pub(crate) fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

pub(crate) fn ensure_private_dir(dir: &Path) -> AppResult<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    // `mode` only applies to directories this call created; tighten an existing one too.
    fs::set_permissions(dir, Permissions::from_mode(0o700))?;
    Ok(())
}

pub(crate) fn set_private_file(path: &Path) -> AppResult<()> {
    fs::set_permissions(path, Permissions::from_mode(0o600))?;
    Ok(())
}

pub(crate) fn remove_if_exists(path: &Path) -> AppResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::io(error)),
    }
}

fn private_subdir(data_dir: &Path, name: &str) -> AppResult<PathBuf> {
    let dir = data_dir.join(name);
    ensure_private_dir(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_pointer_round_trips_and_resets_to_default() {
        let data = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        assert_eq!(database_dir_in(data.path()).unwrap(), data.path());

        write_location(data.path(), Some(elsewhere.path())).unwrap();
        assert_eq!(database_dir_in(data.path()).unwrap(), elsewhere.path());

        write_location(data.path(), Some(data.path())).unwrap();
        assert!(!data.path().join(LOCATION_FILE).exists());
        assert_eq!(database_dir_in(data.path()).unwrap(), data.path());
    }

    #[test]
    fn private_dirs_are_owner_only() {
        let data = tempfile::tempdir().unwrap();
        let backups = backups_dir_in(data.path()).unwrap();
        let mode = fs::metadata(&backups).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
