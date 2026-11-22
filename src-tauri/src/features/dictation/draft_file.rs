// FilePath: src-tauri/src/features/dictation/draft_file.rs
//! The audio file a recording's drafts are written to while it formats as the user speaks
//! (requirements LT-5). Removing it must win against a write still in flight: an aborted
//! worker leaves its blocking write running, so a remove that only deleted the file could run
//! before that write lands and leave the draft behind. Both take the same lock, and a write
//! after the remove does nothing. Drafts left by a quit or a crash are swept at launch.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sv_audio::encode_wav;
use sv_domain::{AppError, AppResult};

use crate::state::lock;

/// One recording's draft file, shared by the worker that writes it and the recording that
/// removes it.
#[derive(Debug, Clone)]
pub(super) struct DraftFile {
    path: PathBuf,
    /// Set once the file was removed; held while writing or removing.
    removed: Arc<Mutex<bool>>,
}

impl DraftFile {
    /// Recording `id`'s draft in `dir`, one file overwritten by each draft.
    pub(super) fn new(dir: &Path, id: u64) -> Self {
        Self {
            path: dir.join(format!("{DRAFT_PREFIX}{id}{DRAFT_SUFFIX}")),
            removed: Arc::new(Mutex::new(false)),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    /// Writes `samples` as the draft, off the async runtime; refused once the file was removed.
    pub(super) async fn write(&self, samples: &[i16]) -> AppResult<()> {
        let file = self.clone();
        let wav = encode_wav(samples);
        tauri::async_runtime::spawn_blocking(move || file.write_now(&wav))
            .await
            .map_err(AppError::other)?
    }

    fn write_now(&self, wav: &[u8]) -> AppResult<()> {
        let removed = lock(&self.removed);
        if *removed {
            return Err(AppError::other("the recording ended"));
        }
        std::fs::write(&self.path, wav).map_err(AppError::io)
    }

    /// Removes the draft, after any write in flight; later writes do nothing.
    pub(super) fn remove(&self) {
        let mut removed = lock(&self.removed);
        *removed = true;
        remove_quietly(&self.path);
    }
}

const DRAFT_PREFIX: &str = "draft-";
const DRAFT_SUFFIX: &str = ".wav";

/// Removes every draft in `dir`: run at launch, before any recording, for the drafts a quit or
/// a crash left behind.
pub(crate) fn sweep(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "could not look for leftover dictation drafts");
            }
            return;
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(DRAFT_PREFIX) && name.ends_with(DRAFT_SUFFIX) {
            remove_quietly(&entry.path());
        }
    }
}

fn remove_quietly(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(%error, "could not remove a dictation draft");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temp dir, removed by the test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sv-draft-file-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_write_after_the_remove_leaves_no_file() {
        let dir = scratch("late-write");
        let file = DraftFile::new(&dir, 7);
        file.write_now(b"first").expect("written");
        assert!(file.path().exists());
        file.remove();
        assert!(!file.path().exists());
        // The aborted worker's write lands after the stop removed the file.
        assert!(file.write_now(b"late").is_err());
        assert!(!file.path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sweep_removes_only_drafts() {
        let dir = scratch("sweep");
        for name in [
            "draft-1.wav",
            "draft-22.wav",
            "session-3-1.wav",
            "draft-notes.txt",
        ] {
            std::fs::write(dir.join(name), b"x").expect("written");
        }
        sweep(&dir);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .expect("listed")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["draft-notes.txt", "session-3-1.wav"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_folder_is_nothing_to_sweep() {
        sweep(&std::env::temp_dir().join("sv-draft-file-does-not-exist"));
    }
}
