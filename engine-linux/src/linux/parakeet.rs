// FilePath: engine-linux/src/linux/parakeet.rs
//! Parakeet TDT transcription on sherpa-onnx's offline transducer recognizer (ONNX Runtime on
//! the CPU). One model stays resident after `preload` or the first dictation. Parakeet writes
//! punctuated, cased text and detects the language itself, so `language`, `languages` and
//! `prompt` have nothing to steer; it reports no confidence, so Smart Select's quality numbers
//! stay empty. A cancelled request stops waiting at once; the clip in flight finishes decoding
//! in the background, as Whisper's does.

use std::sync::Arc;

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

use super::log;
use super::models::{self, Runtime, Store, MAC_ONLY};
use super::transcript::{read_samples, Segment, TranscribeParams, Transcript};

/// ONNX Runtime stops gaining past about eight threads on a desktop CPU.
const MAX_THREADS: usize = 8;
const SAMPLE_RATE: i32 = 16_000;
/// A moment of silence decoded after loading, so the first dictation finds warm buffers.
const WARM_UP_SAMPLES: usize = SAMPLE_RATE as usize / 2;

/// The order `models.rs` lists a Parakeet model's files in.
const ENCODER: usize = 0;
const DECODER: usize = 1;
const JOINER: usize = 2;
const TOKENS: usize = 3;

struct Loaded {
    id: &'static str,
    recognizer: OfflineRecognizer,
}

/// The resident model, shared by every transcription.
#[derive(Default)]
pub(crate) struct Resident {
    loaded: tokio::sync::Mutex<Option<Arc<Loaded>>>,
}

impl std::fmt::Debug for Resident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resident").finish_non_exhaustive()
    }
}

impl Resident {
    /// Loads `id` unless it is resident already; the lock makes concurrent callers wait for
    /// one load instead of loading twice. Another Parakeet model replaces the resident one.
    pub(crate) async fn preload(&self, store: &Store, id: &str) -> Result<(), String> {
        self.load(store, id).await.map(drop)
    }

    async fn load(&self, store: &Store, id: &str) -> Result<Arc<Loaded>, String> {
        let model = models::find(id)
            .filter(|model| model.runtime == Runtime::Parakeet)
            .ok_or(MAC_ONLY)?;
        let mut loaded = self.loaded.lock().await;
        if let Some(resident) = loaded.as_ref().filter(|resident| resident.id == model.id) {
            return Ok(Arc::clone(resident));
        }
        *loaded = None;
        let path = |index| {
            store
                .ready_path(model, index)
                .map(|path| path.to_string_lossy().into_owned())
                .ok_or_else(|| {
                    format!("{id} is not downloaded; download it in Settings → Transcription")
                })
        };
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(path(ENCODER)?),
            decoder: Some(path(DECODER)?),
            joiner: Some(path(JOINER)?),
        };
        config.model_config.tokens = Some(path(TOKENS)?);
        config.model_config.model_type = Some("nemo_transducer".to_owned());
        config.model_config.num_threads = i32::try_from(threads()).unwrap_or(4);
        log::info(&format!("loading {id} with sherpa-onnx"));
        let started = std::time::Instant::now();
        let recognizer = tokio::task::spawn_blocking(move || {
            let recognizer = OfflineRecognizer::create(&config)
                .ok_or("could not load the model; delete it and download it again")?;
            recognize(&recognizer, &vec![0.0; WARM_UP_SAMPLES]);
            Ok::<_, String>(recognizer)
        })
        .await
        .map_err(|error| error.to_string())??;
        log::info(&format!(
            "loaded {id} in {:.2} s",
            started.elapsed().as_secs_f32()
        ));
        let resident = Arc::new(Loaded {
            id: model.id,
            recognizer,
        });
        *loaded = Some(Arc::clone(&resident));
        Ok(resident)
    }

    pub(crate) async fn unload(&self) {
        *self.loaded.lock().await = None;
    }

    pub(crate) async fn transcribe(
        &self,
        store: &Store,
        params: TranscribeParams,
    ) -> Result<Transcript, String> {
        let samples = read_samples(&params).await?;
        let resident = self.load(store, &params.model).await?;
        tokio::task::spawn_blocking(move || {
            let audio: Vec<f32> = samples.iter().map(|&s| f32::from(s) / 32_768.0).collect();
            let result = recognize(&resident.recognizer, &audio)
                .ok_or("sherpa-onnx returned no result for the recording")?;
            let mut transcript = Transcript {
                text: result.text.trim().to_owned(),
                ..Transcript::default()
            };
            if params.timestamps {
                transcript.segments = segments(
                    &result.tokens,
                    result.timestamps.as_deref().unwrap_or_default(),
                    result.durations.as_deref().unwrap_or_default(),
                );
            }
            Ok(transcript)
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

fn recognize(
    recognizer: &OfflineRecognizer,
    audio: &[f32],
) -> Option<sherpa_onnx::OfflineRecognizerResult> {
    let stream = recognizer.create_stream();
    stream.accept_waveform(SAMPLE_RATE, audio);
    recognizer.decode(&stream);
    stream.get_result()
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .min(MAX_THREADS)
}

/// Sentences from the token stream: each token's start time (seconds) and, when the model
/// reports them, durations. A sentence ends at a token that ends in `.`, `?` or `!`.
fn segments(tokens: &[String], starts: &[f32], durations: &[f32]) -> Vec<Segment> {
    let millis = |seconds: f32| (f64::from(seconds) * 1000.0).round() as i64;
    let mut segments = Vec::new();
    let mut text = String::new();
    let mut start = None;
    for (index, (token, &at)) in tokens.iter().zip(starts).enumerate() {
        start.get_or_insert(at);
        text.push_str(token);
        let end = durations.get(index).map_or_else(
            || starts.get(index + 1).copied().unwrap_or(at),
            |duration| at + duration,
        );
        let last = index + 1 == tokens.len().min(starts.len());
        if last || token.trim_end().ends_with(['.', '?', '!']) {
            let sentence = text.trim();
            if !sentence.is_empty() {
                segments.push(Segment {
                    start_ms: millis(start.unwrap_or(at)),
                    end_ms: millis(end),
                    text: sentence.to_owned(),
                });
            }
            text.clear();
            start = None;
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn splits_timed_tokens_into_sentences() {
        let tokens = strings(&[" Hi", ".", " How", " are", " you", "?"]);
        let starts = [0.0, 0.4, 0.8, 1.0, 1.2, 1.4];
        assert_eq!(
            segments(&tokens, &starts, &[0.4, 0.08, 0.2, 0.2, 0.2, 0.08]),
            vec![
                Segment {
                    start_ms: 0,
                    end_ms: 480,
                    text: "Hi.".to_owned()
                },
                Segment {
                    start_ms: 800,
                    end_ms: 1480,
                    text: "How are you?".to_owned()
                },
            ]
        );
        let unpunctuated = segments(&strings(&[" one", " two"]), &[0.0, 0.5], &[]);
        assert_eq!(unpunctuated.len(), 1);
        assert_eq!(unpunctuated[0].text, "one two");
        assert_eq!(unpunctuated[0].end_ms, 500);
    }

    #[test]
    fn refuses_models_it_does_not_run() {
        let dir = std::env::temp_dir();
        let store = Store::new(dir);
        let resident = Resident::default();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let whisper = runtime.block_on(resident.preload(&store, "whisper-large-v3-turbo"));
        assert_eq!(whisper, Err(MAC_ONLY.to_owned()));
    }
}
