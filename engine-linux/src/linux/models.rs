// FilePath: engine-linux/src/linux/models.rs
//! The models the Linux helper runs and their files under `--models-dir`, in the Swift helper's
//! layout: `<id>.partial/` while downloading (an interrupted download resumes), `<id>/` with a
//! `.simple-voice-complete` marker once every byte arrived and matched the pinned SHA-256.
//! Only Whisper Turbo runs on Linux; every other catalog model reports `unsupported` with the
//! reason, so Settings labels it instead of offering a download that could never run.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sha2::{Digest, Sha256};
use sv_domain::models::{model_name, WHISPER_TURBO};
use sv_domain::ModelStatus;
use tokio::io::AsyncWriteExt;

use super::lock;
use super::output::Progress;

const MARKER: &str = ".simple-voice-complete";
/// Report at most this often (fraction of the file) so the progress bar moves without flooding
/// the protocol stream.
const PROGRESS_STEP: f64 = 0.005;

/// A model file pinned to one repository revision and its SHA-256.
#[derive(Debug)]
pub(crate) struct LinuxModel {
    pub(crate) id: &'static str,
    repo: &'static str,
    revision: &'static str,
    pub(crate) file: &'static str,
    bytes: u64,
    sha256: &'static str,
    license: &'static str,
    source: &'static str,
}

/// Whisper large-v3-turbo in whisper.cpp's format, 5-bit quantised: the same model the macOS
/// helper runs on Core ML, a third of the size, and fast enough on a desktop CPU.
const WHISPER_TURBO_GGML: LinuxModel = LinuxModel {
    id: WHISPER_TURBO,
    repo: "ggerganov/whisper.cpp",
    revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
    file: "ggml-large-v3-turbo-q5_0.bin",
    bytes: 574_041_195,
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    license: "MIT",
    source: "ggerganov/whisper.cpp (openai/whisper-large-v3-turbo, q5_0)",
};

const MODELS: &[LinuxModel] = &[WHISPER_TURBO_GGML];

pub(crate) const MAC_ONLY: &str = "Runs on macOS only. On Linux, Simple Voice transcribes with \
    Whisper Turbo; formatting and edit mode can use Groq.";

pub(crate) fn find(id: &str) -> Option<&'static LinuxModel> {
    MODELS.iter().find(|model| model.id == id)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusResult {
    status: ModelStatus,
    size_bytes: Option<u64>,
    reason: Option<&'static str>,
    streaming: bool,
    license: Option<&'static str>,
    source: Option<&'static str>,
    revision: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) struct Store {
    root: PathBuf,
    downloading: Mutex<HashSet<&'static str>>,
}

impl Store {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            downloading: Mutex::default(),
        }
    }

    pub(crate) fn status(&self, id: &str) -> Result<StatusResult, String> {
        let Some(model) = find(id) else {
            return model_name(id)
                .map(|_| StatusResult {
                    status: ModelStatus::Unsupported,
                    size_bytes: None,
                    reason: Some(MAC_ONLY),
                    streaming: false,
                    license: None,
                    source: None,
                    revision: None,
                })
                .ok_or_else(|| format!("unknown model `{id}`"));
        };
        let (status, size_bytes) = if lock(&self.downloading).contains(model.id) {
            (ModelStatus::Downloading, Some(model.bytes))
        } else if let Some(path) = self.ready_file(model) {
            let size = std::fs::metadata(path).map_or(model.bytes, |meta| meta.len());
            (ModelStatus::Ready, Some(size))
        } else {
            (ModelStatus::NotDownloaded, Some(model.bytes))
        };
        Ok(StatusResult {
            status,
            size_bytes,
            reason: None,
            streaming: false,
            license: Some(model.license),
            source: Some(model.source),
            revision: Some(model.revision),
        })
    }

    /// The model file, when the download completed.
    pub(crate) fn ready_file(&self, model: &LinuxModel) -> Option<PathBuf> {
        let dir = self.root.join(model.id);
        dir.join(MARKER)
            .is_file()
            .then(|| dir.join(model.file))
            .filter(|file| file.is_file())
    }

    pub(crate) async fn download(&self, id: &str, progress: &Progress) -> Result<(), String> {
        let model = find(id).ok_or(MAC_ONLY)?;
        if self.ready_file(model).is_some() {
            return Ok(());
        }
        if !lock(&self.downloading).insert(model.id) {
            return Err(format!("{id} is already downloading"));
        }
        let result = self.fetch(model, progress).await;
        lock(&self.downloading).remove(model.id);
        result
    }

    async fn fetch(&self, model: &LinuxModel, progress: &Progress) -> Result<(), String> {
        let partial = self.root.join(format!("{}.partial", model.id));
        tokio::fs::create_dir_all(&partial)
            .await
            .map_err(|error| format!("cannot create {}: {error}", partial.display()))?;
        let part = partial.join(model.file);
        let mut have = tokio::fs::metadata(&part)
            .await
            .map_or(0, |meta| meta.len());
        if have > model.bytes {
            have = 0;
        }
        if have < model.bytes {
            have = receive(model, &part, have, progress).await?;
        }
        if have != model.bytes {
            return Err(format!(
                "the download of {} stopped at {have} of {} bytes",
                model.file, model.bytes
            ));
        }
        progress.report(1.0, "Checking the download");
        let digest = sha256_of(&part).await?;
        if digest != model.sha256 {
            remove_file(&part);
            return Err(format!(
                "{} does not match its published checksum; the file was deleted, download it again",
                model.file
            ));
        }
        tokio::fs::write(partial.join(MARKER), model.bytes.to_string())
            .await
            .map_err(|error| format!("cannot finish the download: {error}"))?;
        let done = self.root.join(model.id);
        remove_dir(&done);
        tokio::fs::rename(&partial, &done)
            .await
            .map_err(|error| format!("cannot finish the download: {error}"))
    }

    pub(crate) fn delete(&self, id: &str) -> Result<(), String> {
        let model = find(id).ok_or(MAC_ONLY)?;
        if lock(&self.downloading).contains(model.id) {
            return Err(format!("{id} is still downloading"));
        }
        for dir in [
            self.root.join(model.id),
            self.root.join(format!("{}.partial", model.id)),
        ] {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cannot delete {}: {error}", dir.display())),
            }
        }
        Ok(())
    }
}

/// Appends the rest of the file from byte `have` on; returns the bytes on disk afterwards.
async fn receive(
    model: &LinuxModel,
    part: &Path,
    mut have: u64,
    progress: &Progress,
) -> Result<u64, String> {
    let url = format!(
        "https://huggingface.co/{}/resolve/{}/{}",
        model.repo, model.revision, model.file
    );
    let client = reqwest::Client::new();
    let mut request = client.get(&url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut response = request
        .send()
        .await
        .map_err(|error| format!("could not reach Hugging Face: {error}"))?;
    let append = match response.status().as_u16() {
        206 => true,
        200 => {
            have = 0;
            false
        }
        status => return Err(format!("Hugging Face answered HTTP {status} for {url}")),
    };
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part)
        .await
        .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
    let name = model_name(model.id).unwrap_or(model.id);
    let total = model.bytes as f64;
    let mut reported = -1.0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("the download was interrupted: {error}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
        have = have.saturating_add(chunk.len() as u64);
        let fraction = have as f64 / total;
        if fraction - reported >= PROGRESS_STEP {
            reported = fraction;
            progress.report(fraction * 0.99, &format!("Downloading {name}"));
        }
    }
    file.flush()
        .await
        .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
    Ok(have)
}

async fn sha256_of(path: &Path) -> Result<String, String> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path).map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn remove_file(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        super::log::error(&format!("cannot delete {}: {error}", path.display()));
    }
}

fn remove_dir(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => super::log::error(&format!("cannot delete {}: {error}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_domain::models::{APPLE_INTELLIGENCE, PARAKEET_TDT_V3};

    #[test]
    fn whisper_is_downloadable_and_mac_models_are_labelled() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        let whisper = store.status(WHISPER_TURBO).unwrap();
        assert_eq!(whisper.status, ModelStatus::NotDownloaded);
        assert_eq!(whisper.size_bytes, Some(WHISPER_TURBO_GGML.bytes));
        for id in [PARAKEET_TDT_V3, APPLE_INTELLIGENCE] {
            let status = store.status(id).unwrap();
            assert_eq!(status.status, ModelStatus::Unsupported);
            assert_eq!(status.reason, Some(MAC_ONLY));
        }
        assert!(store.status("no-such-model").is_err());
    }

    #[test]
    fn a_model_is_ready_only_with_its_marker() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        let model_dir = dir.path().join(WHISPER_TURBO);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join(WHISPER_TURBO_GGML.file), b"weights").unwrap();
        assert!(store.ready_file(&WHISPER_TURBO_GGML).is_none());
        std::fs::write(model_dir.join(MARKER), b"7").unwrap();
        assert!(store.ready_file(&WHISPER_TURBO_GGML).is_some());
        assert_eq!(
            store.status(WHISPER_TURBO).unwrap().status,
            ModelStatus::Ready
        );
        store.delete(WHISPER_TURBO).unwrap();
        assert!(!model_dir.exists());
    }

    #[tokio::test]
    async fn hashes_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_of(&path).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
