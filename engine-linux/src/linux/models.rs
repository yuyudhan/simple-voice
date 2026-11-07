// FilePath: engine-linux/src/linux/models.rs
//! The models the Linux helper runs and their files under `--models-dir`, in the Swift helper's
//! layout: `<id>.partial/` while downloading (an interrupted download resumes), `<id>/` with a
//! `.simple-voice-complete` marker once every file arrived and matched its pinned SHA-256.
//! Parakeet TDT runs on sherpa-onnx and Whisper Turbo on whisper.cpp; every other catalog
//! model reports `unsupported` with the reason, so Settings labels it instead of offering a
//! download that could never run.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sha2::{Digest, Sha256};
use sv_domain::models::{model_name, PARAKEET_TDT_V2, PARAKEET_TDT_V3, WHISPER_TURBO};
use sv_domain::ModelStatus;
use tokio::io::AsyncWriteExt;

use super::lock;
use super::output::Progress;

const MARKER: &str = ".simple-voice-complete";
/// Report at most this often (fraction of the file) so the progress bar moves without flooding
/// the protocol stream.
const PROGRESS_STEP: f64 = 0.005;

/// The library that runs a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Runtime {
    Parakeet,
    Whisper,
}

/// One file of a model, pinned by size and SHA-256.
#[derive(Debug)]
pub(crate) struct ModelFile {
    pub(crate) name: &'static str,
    bytes: u64,
    sha256: &'static str,
}

/// A model pinned to one Hugging Face repository revision.
#[derive(Debug)]
pub(crate) struct LinuxModel {
    pub(crate) id: &'static str,
    pub(crate) runtime: Runtime,
    repo: &'static str,
    revision: &'static str,
    pub(crate) files: &'static [ModelFile],
    license: &'static str,
    source: &'static str,
}

impl LinuxModel {
    fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

/// Parakeet TDT 0.6B v3 exported to ONNX by sherpa-onnx, 8-bit: encoder, decoder, joiner and
/// tokens. The same model the macOS helper runs on Core ML.
const PARAKEET_V3_ONNX: LinuxModel = LinuxModel {
    id: PARAKEET_TDT_V3,
    runtime: Runtime::Parakeet,
    repo: "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8",
    revision: "ed5d745623472f97cfc2e6c2916ee248798fc5a8",
    files: &[
        ModelFile {
            name: "encoder.int8.onnx",
            bytes: 652_282_294,
            sha256: "1afbe69c1ffdabee94755f42e4daf1d93549131a3da11356bda84fcef0f2c479",
        },
        ModelFile {
            name: "decoder.int8.onnx",
            bytes: 11_845_274,
            sha256: "0222a86147299d4dee90edf95b6d13827edb42ad8b7c9cf0b2582f030937c0b5",
        },
        ModelFile {
            name: "joiner.int8.onnx",
            bytes: 6_355_277,
            sha256: "cbd02f3696e9944970111fb439d9c3fd365a80374565f13f2b9c7007d253fc98",
        },
        ModelFile {
            name: "tokens.txt",
            bytes: 93_939,
            sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
        },
    ],
    license: "CC-BY-4.0",
    source: "k2-fsa/sherpa-onnx (nvidia/parakeet-tdt-0.6b-v3, int8)",
};

/// Parakeet TDT 0.6B v2 (English) in the same sherpa-onnx layout.
const PARAKEET_V2_ONNX: LinuxModel = LinuxModel {
    id: PARAKEET_TDT_V2,
    runtime: Runtime::Parakeet,
    repo: "csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
    revision: "84b478d541cdbf0c296a7ab5f1e20a6f8f998114",
    files: &[
        ModelFile {
            name: "encoder.int8.onnx",
            bytes: 652_282_309,
            sha256: "92a10bd9c23936ea0566c29f963bc57c2a5df4261b6a8921c39abea66557ca8b",
        },
        ModelFile {
            name: "decoder.int8.onnx",
            bytes: 7_257_753,
            sha256: "4d097bae3bac5c96018d5293ba39f1b657296102b142affda40378f38b020f41",
        },
        ModelFile {
            name: "joiner.int8.onnx",
            bytes: 1_739_080,
            sha256: "77a29f3e5ac7d37074e0af465391b80f314015d5b97f0f90c6c7767f1d56f428",
        },
        ModelFile {
            name: "tokens.txt",
            bytes: 9_384,
            sha256: "ec182b70dd42113aff6c5372c75cac58c952443eb22322f57bbd7f53977d497d",
        },
    ],
    license: "CC-BY-4.0",
    source: "k2-fsa/sherpa-onnx (nvidia/parakeet-tdt-0.6b-v2, int8)",
};

/// Whisper large-v3-turbo in whisper.cpp's format, 5-bit quantised: the same model the macOS
/// helper runs on Core ML, a third of the size, and fast enough on a desktop CPU.
const WHISPER_TURBO_GGML: LinuxModel = LinuxModel {
    id: WHISPER_TURBO,
    runtime: Runtime::Whisper,
    repo: "ggerganov/whisper.cpp",
    revision: "5359861c739e955e79d9a303bcbc70fb988958b1",
    files: &[ModelFile {
        name: "ggml-large-v3-turbo-q5_0.bin",
        bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    }],
    license: "MIT",
    source: "ggerganov/whisper.cpp (openai/whisper-large-v3-turbo, q5_0)",
};

const MODELS: &[LinuxModel] = &[PARAKEET_V3_ONNX, PARAKEET_V2_ONNX, WHISPER_TURBO_GGML];

pub(crate) const MAC_ONLY: &str = "Runs on macOS only. On Linux, Simple Voice transcribes with \
    Parakeet TDT or Whisper Turbo; formatting and edit mode can use Groq.";

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

/// Where pinned model files are fetched from.
const HUGGING_FACE: &str = "https://huggingface.co";

#[derive(Debug)]
pub(crate) struct Store {
    root: PathBuf,
    host: String,
    downloading: Mutex<HashSet<&'static str>>,
}

impl Store {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self {
            root,
            host: HUGGING_FACE.to_owned(),
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
            (ModelStatus::Downloading, Some(model.bytes()))
        } else if let Some(dir) = self.ready_dir(model) {
            let size = model
                .files
                .iter()
                .map(|file| std::fs::metadata(dir.join(file.name)).map_or(file.bytes, |m| m.len()))
                .sum();
            (ModelStatus::Ready, Some(size))
        } else {
            (ModelStatus::NotDownloaded, Some(model.bytes()))
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

    /// The model's directory, when the download completed and every file is there.
    pub(crate) fn ready_dir(&self, model: &LinuxModel) -> Option<PathBuf> {
        let dir = self.root.join(model.id);
        let complete = dir.join(MARKER).is_file()
            && model.files.iter().all(|file| dir.join(file.name).is_file());
        complete.then_some(dir)
    }

    /// File `index` of the model, when the download completed.
    pub(crate) fn ready_path(&self, model: &LinuxModel, index: usize) -> Option<PathBuf> {
        let file = model.files.get(index)?;
        self.ready_dir(model).map(|dir| dir.join(file.name))
    }

    pub(crate) async fn download(&self, id: &str, progress: &Progress) -> Result<(), String> {
        let model = find(id).ok_or(MAC_ONLY)?;
        if self.ready_dir(model).is_some() {
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
        let total = model.bytes();
        let mut before = 0_u64;
        for file in model.files {
            let part = partial.join(file.name);
            let mut have = tokio::fs::metadata(&part)
                .await
                .map_or(0, |meta| meta.len());
            if have > file.bytes {
                have = 0;
            }
            if have < file.bytes {
                let span = Span { before, total };
                have = receive(&self.host, model, file, &part, have, span, progress).await?;
            }
            if have != file.bytes {
                return Err(format!(
                    "the download of {} stopped at {have} of {} bytes",
                    file.name, file.bytes
                ));
            }
            before += file.bytes;
        }
        progress.report(1.0, "Checking the download");
        for file in model.files {
            let part = partial.join(file.name);
            if sha256_of(&part).await? != file.sha256 {
                remove_file(&part);
                return Err(format!(
                    "{} does not match its published checksum; the file was deleted, download it \
                     again",
                    file.name
                ));
            }
        }
        tokio::fs::write(partial.join(MARKER), total.to_string())
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

/// Where one file sits in the model's whole download, for one progress bar across files.
#[derive(Debug, Clone, Copy)]
struct Span {
    before: u64,
    total: u64,
}

/// Appends the rest of `file` from byte `have` on; returns the bytes on disk afterwards.
async fn receive(
    host: &str,
    model: &LinuxModel,
    file: &ModelFile,
    part: &Path,
    mut have: u64,
    span: Span,
    progress: &Progress,
) -> Result<u64, String> {
    let url = format!(
        "{host}/{}/resolve/{}/{}",
        model.repo, model.revision, file.name
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
    let mut out = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part)
        .await
        .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
    let name = model_name(model.id).unwrap_or(model.id);
    let total = span.total as f64;
    let mut reported = -1.0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("the download was interrupted: {error}"))?
    {
        out.write_all(&chunk)
            .await
            .map_err(|error| format!("cannot write {}: {error}", part.display()))?;
        have = have.saturating_add(chunk.len() as u64);
        let fraction = (span.before + have) as f64 / total;
        if fraction - reported >= PROGRESS_STEP {
            reported = fraction;
            progress.report(fraction * 0.99, &format!("Downloading {name}"));
        }
    }
    out.flush()
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
    use sv_domain::models::APPLE_INTELLIGENCE;

    #[test]
    fn parakeet_and_whisper_are_downloadable_and_mac_models_are_labelled() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        let whisper = store.status(WHISPER_TURBO).unwrap();
        assert_eq!(whisper.status, ModelStatus::NotDownloaded);
        assert_eq!(whisper.size_bytes, Some(574_041_195));
        let parakeet = store.status(PARAKEET_TDT_V3).unwrap();
        assert_eq!(parakeet.status, ModelStatus::NotDownloaded);
        assert_eq!(parakeet.size_bytes, Some(670_576_784));
        assert_eq!(
            find(PARAKEET_TDT_V2).map(|m| m.runtime),
            Some(Runtime::Parakeet)
        );
        let apple = store.status(APPLE_INTELLIGENCE).unwrap();
        assert_eq!(apple.status, ModelStatus::Unsupported);
        assert_eq!(apple.reason, Some(MAC_ONLY));
        assert!(store.status("no-such-model").is_err());
    }

    #[test]
    fn a_model_is_ready_only_with_its_marker_and_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_path_buf());
        let model_dir = dir.path().join(PARAKEET_TDT_V3);
        std::fs::create_dir_all(&model_dir).unwrap();
        std::fs::write(model_dir.join(MARKER), b"7").unwrap();
        let (last, rest) = PARAKEET_V3_ONNX.files.split_last().unwrap();
        for file in rest {
            std::fs::write(model_dir.join(file.name), b"weights").unwrap();
        }
        assert!(store.ready_dir(&PARAKEET_V3_ONNX).is_none());
        std::fs::write(model_dir.join(last.name), b"tokens").unwrap();
        assert_eq!(store.ready_dir(&PARAKEET_V3_ONNX), Some(model_dir.clone()));
        assert_eq!(
            store.ready_path(&PARAKEET_V3_ONNX, 0),
            Some(model_dir.join("encoder.int8.onnx"))
        );
        assert_eq!(
            store.status(PARAKEET_TDT_V3).unwrap().status,
            ModelStatus::Ready
        );
        store.delete(PARAKEET_TDT_V3).unwrap();
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

    const TWO_FILES: LinuxModel = LinuxModel {
        id: PARAKEET_TDT_V3,
        runtime: Runtime::Parakeet,
        repo: "org/repo",
        revision: "rev",
        files: &[
            ModelFile {
                name: "a.bin",
                bytes: 3,
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            },
            ModelFile {
                name: "b.txt",
                bytes: 5,
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            },
        ],
        license: "MIT",
        source: "test",
    };

    /// A local HTTP server answering `/org/repo/resolve/rev/<name>` with `files`, honouring
    /// `Range: bytes=N-` as Hugging Face does.
    async fn serve(files: &'static [(&'static str, &'static [u8])]) -> String {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = vec![0; 4096];
                let read = socket.read(&mut request).await.unwrap();
                let request = String::from_utf8_lossy(&request[..read]).into_owned();
                let path = request.split_whitespace().nth(1).unwrap().to_owned();
                let from: usize = request
                    .lines()
                    .find_map(|line| line.strip_prefix("range: bytes="))
                    .or_else(|| {
                        request
                            .lines()
                            .find_map(|l| l.strip_prefix("Range: bytes="))
                    })
                    .and_then(|range| range.trim_end_matches('-').parse().ok())
                    .unwrap_or(0);
                let body = files
                    .iter()
                    .find(|(name, _)| path == format!("/org/repo/resolve/rev/{name}"))
                    .map(|(_, body)| &body[from..]);
                let head = match body {
                    Some(body) if from > 0 => {
                        format!("206 Partial Content\r\ncontent-length: {}", body.len())
                    }
                    Some(body) => format!("200 OK\r\ncontent-length: {}", body.len()),
                    None => "404 Not Found\r\ncontent-length: 0".to_owned(),
                };
                let reply = format!("HTTP/1.1 {head}\r\nconnection: close\r\n\r\n");
                socket.write_all(reply.as_bytes()).await.unwrap();
                socket.write_all(body.unwrap_or_default()).await.unwrap();
            }
        });
        host
    }

    fn store_at(root: &Path, host: String) -> Store {
        Store {
            host,
            ..Store::new(root.to_path_buf())
        }
    }

    fn progress() -> Progress {
        Progress::new(1, std::sync::Arc::default())
    }

    #[tokio::test]
    async fn downloads_every_file_resumes_and_marks_the_model_complete() {
        let host = serve(&[("a.bin", b"abc"), ("b.txt", b"hello")]).await;
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path(), host);
        // An interrupted earlier download left the first file half there.
        let partial = dir.path().join(format!("{PARAKEET_TDT_V3}.partial"));
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("a.bin"), b"ab").unwrap();
        store.fetch(&TWO_FILES, &progress()).await.unwrap();
        let done = store.ready_dir(&TWO_FILES).unwrap();
        assert_eq!(std::fs::read(done.join("a.bin")).unwrap(), b"abc");
        assert_eq!(std::fs::read(done.join("b.txt")).unwrap(), b"hello");
        assert_eq!(std::fs::read_to_string(done.join(MARKER)).unwrap(), "8");
        assert!(!partial.exists());
    }

    #[tokio::test]
    async fn a_file_that_fails_its_checksum_is_deleted_and_nothing_is_ready() {
        let host = serve(&[("a.bin", b"abc"), ("b.txt", b"jello")]).await;
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path(), host);
        let error = store.fetch(&TWO_FILES, &progress()).await.unwrap_err();
        assert!(error.contains("b.txt does not match"), "{error}");
        assert!(store.ready_dir(&TWO_FILES).is_none());
        let partial = dir.path().join(format!("{PARAKEET_TDT_V3}.partial"));
        assert!(partial.join("a.bin").is_file());
        assert!(!partial.join("b.txt").exists());
    }

    #[tokio::test]
    async fn a_missing_file_fails_the_download() {
        let host = serve(&[("a.bin", b"abc")]).await;
        let dir = tempfile::tempdir().unwrap();
        let error = store_at(dir.path(), host)
            .fetch(&TWO_FILES, &progress())
            .await
            .unwrap_err();
        assert!(error.contains("HTTP 404"), "{error}");
    }
}
