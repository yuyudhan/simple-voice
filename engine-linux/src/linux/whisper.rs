// FilePath: engine-linux/src/linux/whisper.rs
//! Whisper transcription on whisper.cpp. One model stays resident after `preload` or the first
//! dictation; `language` pins the language, `languages` restricts detection to the user's
//! languages, and `prompt` conditions the decoder on the personal dictionary, as the Swift
//! helper's WhisperKit path does. A cancelled request stops waiting at once; the clip in flight
//! finishes decoding in the background (whisper.cpp's abort callback breaks its language
//! detection, and a dictation decodes in a few seconds).
//!
//! Decoder states are kept between requests: whisper.cpp encodes the language-detection pass
//! with the context size of the state's previous decode, so a fresh state would detect over a
//! full 30 s window and a warmed one detects over seconds. `preload` warms one.

use std::sync::{Arc, Mutex};

use sv_domain::models::WHISPER_TURBO;
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use super::models::{self, Store, MAC_ONLY};
use super::transcript::{read_samples, Segment, TranscribeParams, Transcript};
use super::{lock, log};

/// whisper.cpp stops gaining past about eight threads on a desktop CPU.
const MAX_THREADS: usize = 8;
/// Candidates sampled when a decode falls back to a higher temperature, as OpenAI's reference.
const BEST_OF: i32 = 5;
/// Whisper needs a little audio to decode; shorter clips are padded with silence.
const MIN_SAMPLES: usize = 2 * 16_000;
/// The encoder sees 30 s windows of 1500 frames (50 per second of audio).
const FRAMES_PER_SECOND: usize = 50;
const WINDOW_FRAMES: usize = 1500;
/// Frames of encoder context past the end of the speech: Whisper writes the end of a clip
/// unreliably when its context stops right where the audio does.
const CONTEXT_MARGIN_FRAMES: usize = 100;

/// The loaded model and its idle decoder states.
struct Loaded {
    context: WhisperContext,
    idle: Mutex<Vec<WhisperState>>,
}

impl Loaded {
    fn take_state(&self) -> Result<WhisperState, String> {
        match lock(&self.idle).pop() {
            Some(state) => Ok(state),
            None => self
                .context
                .create_state()
                .map_err(|error| format!("whisper: {error}")),
        }
    }

    fn put_state(&self, state: WhisperState) {
        lock(&self.idle).push(state);
    }
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
    /// Loads `id` unless it is resident already, and warms a decoder state; the lock makes
    /// concurrent callers wait for one load instead of loading twice.
    pub(crate) async fn preload(&self, store: &Store, id: &str) -> Result<(), String> {
        self.load(store, id).await.map(drop)
    }

    async fn load(&self, store: &Store, id: &str) -> Result<Arc<Loaded>, String> {
        let model = models::find(id).ok_or(MAC_ONLY)?;
        let mut loaded = self.loaded.lock().await;
        if let Some(resident) = loaded.as_ref() {
            return Ok(Arc::clone(resident));
        }
        let path = store.ready_path(model, 0).ok_or_else(|| {
            format!("{id} is not downloaded; download it in Settings → Transcription")
        })?;
        log::info(&format!("loading {id} from {}", path.display()));
        let started = std::time::Instant::now();
        let resident = tokio::task::spawn_blocking(move || {
            whisper_rs::install_logging_hooks();
            let context =
                WhisperContext::new_with_params(&path, WhisperContextParameters::default())
                    .map_err(|error| format!("could not load the model: {error}"))?;
            let resident = Loaded {
                context,
                idle: Mutex::default(),
            };
            warm_up(&resident)?;
            Ok::<_, String>(resident)
        })
        .await
        .map_err(|error| error.to_string())??;
        log::info(&format!(
            "loaded {id} in {:.2} s",
            started.elapsed().as_secs_f32()
        ));
        let resident = Arc::new(resident);
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
        if params.model != WHISPER_TURBO {
            return Err(format!("{}: {MAC_ONLY}", params.model));
        }
        let samples = read_samples(&params).await?;
        let resident = self.load(store, &params.model).await?;
        tokio::task::spawn_blocking(move || {
            let mut state = resident.take_state()?;
            let eot = resident.context.token_eot();
            let result = decode(&mut state, &samples, &params, eot);
            resident.put_state(state);
            result
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

/// Decodes a moment of silence, which sets up the state's buffers and a short detection
/// context before the first dictation needs them.
fn warm_up(resident: &Loaded) -> Result<(), String> {
    let mut state = resident.take_state()?;
    let silence = vec![0.0_f32; MIN_SAMPLES];
    let request = TranscribeParams {
        model: WHISPER_TURBO.to_owned(),
        wav_path: String::new(),
        language: None,
        languages: Vec::new(),
        prompt: String::new(),
        timestamps: false,
    };
    let result = run(&mut state, &silence, Some("en"), &request, threads());
    resident.put_state(state);
    result
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .min(MAX_THREADS)
}

fn decode(
    state: &mut WhisperState,
    samples: &[i16],
    request: &TranscribeParams,
    eot: i32,
) -> Result<Transcript, String> {
    let mut audio: Vec<f32> = samples.iter().map(|&s| f32::from(s) / 32_768.0).collect();
    if audio.len() < MIN_SAMPLES {
        audio.resize(MIN_SAMPLES, 0.0);
    }
    let threads = threads();
    match pinned_language(request) {
        Some(language) => run(state, &audio, Some(&language), request, threads)?,
        None => {
            run(state, &audio, None, request, threads)?;
            let detected = whisper_rs::get_lang_str(state.full_lang_id_from_state());
            let allowed = known(&request.languages);
            // Detection heard a language the user does not speak: decode again in the first
            // of theirs, as WhisperKit's restricted detection would have chosen among them.
            if let Some(first) = allowed.first() {
                if !detected.is_some_and(|heard| allowed.contains(&heard)) {
                    run(state, &audio, Some(first), request, threads)?;
                }
            }
        }
    }
    collect(state, request.timestamps, eot)
}

/// One whisper.cpp pass over `audio`, in `language` or detecting it when `None`.
fn run(
    state: &mut WhisperState,
    audio: &[f32],
    language: Option<&str>,
    request: &TranscribeParams,
    threads: usize,
) -> Result<(), String> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: BEST_OF });
    params.set_n_threads(i32::try_from(threads).unwrap_or(4));
    params.set_language(Some(language.unwrap_or("auto")));
    params.set_translate(false);
    params.set_no_context(true);
    params.set_no_timestamps(!request.timestamps);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    if !request.prompt.trim().is_empty() {
        params.set_initial_prompt(request.prompt.trim());
    }
    if let Some(frames) = audio_context(audio.len(), request.timestamps) {
        params.set_audio_ctx(frames);
    }
    state
        .full(params, audio)
        .map(drop)
        .map_err(|error| format!("whisper: {error}"))
}

/// The transcript, its timed segments when asked for, and Smart Select's quality numbers. Tokens
/// from `eot` up are special (end of text, timestamps) and carry no recognition confidence.
fn collect(state: &WhisperState, timestamps: bool, eot: i32) -> Result<Transcript, String> {
    let mut transcript = Transcript::default();
    let mut texts = Vec::new();
    let (mut logprob_sum, mut no_speech_sum, mut tokens) = (0.0_f32, 0.0_f32, 0_u32);
    for index in 0..state.full_n_segments() {
        let Some(segment) = state.get_segment(index) else {
            continue;
        };
        let text = segment
            .to_str_lossy()
            .map_err(|error| format!("whisper: {error}"))?
            .trim()
            .to_owned();
        if text.is_empty() || is_annotation(&text) {
            continue;
        }
        let mut segment_tokens = 0_u32;
        for token_index in 0..segment.n_tokens() {
            let Some(token) = segment.get_token(token_index) else {
                continue;
            };
            if token.token_id() >= eot {
                continue;
            }
            logprob_sum += token.token_data().plog;
            segment_tokens += 1;
        }
        tokens += segment_tokens;
        no_speech_sum += segment.no_speech_probability() * segment_tokens as f32;
        if timestamps {
            transcript.segments.push(Segment {
                start_ms: segment.start_timestamp().saturating_mul(10),
                end_ms: segment.end_timestamp().saturating_mul(10),
                text: text.clone(),
            });
        }
        texts.push(text);
    }
    transcript.text = texts.join(" ");
    transcript.language =
        whisper_rs::get_lang_str(state.full_lang_id_from_state()).map(str::to_owned);
    if tokens > 0 {
        transcript.quality.avg_logprob = Some(logprob_sum / tokens as f32);
        transcript.quality.no_speech_prob = Some(no_speech_sum / tokens as f32);
    }
    Ok(transcript)
}

/// The language to decode in without detection: the pinned one, or the only allowed one.
fn pinned_language(request: &TranscribeParams) -> Option<String> {
    let pinned = request
        .language
        .as_deref()
        .map(base_language)
        .filter(|language| whisper_rs::get_lang_id(language).is_some())
        .map(str::to_owned);
    pinned.or_else(|| match known(&request.languages).as_slice() {
        [only] => Some((*only).to_owned()),
        _ => None,
    })
}

/// The encoder context for a clip of `samples`. A dictation is seconds long, and encoding only
/// what it holds instead of a padded 30 s window cuts the CPU time several-fold (the encoder is
/// most of Whisper Turbo's work). Long and timed (meeting) audio keeps the full window.
fn audio_context(samples: usize, timestamps: bool) -> Option<i32> {
    let frames = samples.div_ceil(16_000 / FRAMES_PER_SECOND) + CONTEXT_MARGIN_FRAMES;
    if timestamps || frames >= WINDOW_FRAMES {
        return None;
    }
    i32::try_from(frames).ok()
}

/// The base languages of `tags` that Whisper knows, deduplicated, in order.
fn known(tags: &[String]) -> Vec<&str> {
    let mut languages: Vec<&str> = Vec::new();
    for language in tags.iter().map(|tag| base_language(tag)) {
        if whisper_rs::get_lang_id(language).is_some() && !languages.contains(&language) {
            languages.push(language);
        }
    }
    languages
}

/// `hi-Latn` → `hi`: Whisper writes romanised variants under their base language.
fn base_language(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or(tag)
}

/// whisper.cpp's non-speech markers such as `[BLANK_AUDIO]` or `(music)`.
fn is_annotation(text: &str) -> bool {
    (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('(') && text.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(language: Option<&str>, languages: &[&str]) -> TranscribeParams {
        TranscribeParams {
            model: WHISPER_TURBO.to_owned(),
            wav_path: String::new(),
            language: language.map(str::to_owned),
            languages: languages.iter().map(|tag| (*tag).to_owned()).collect(),
            prompt: String::new(),
            timestamps: false,
        }
    }

    #[test]
    fn pins_the_language_when_one_is_known() {
        assert_eq!(
            pinned_language(&request(Some("en"), &[])).as_deref(),
            Some("en")
        );
        assert_eq!(
            pinned_language(&request(None, &["hi-Latn"])).as_deref(),
            Some("hi")
        );
        assert_eq!(pinned_language(&request(None, &["en", "hi"])), None);
        assert_eq!(
            pinned_language(&request(None, &["hi", "hi-Latn"])).as_deref(),
            Some("hi")
        );
        assert_eq!(pinned_language(&request(None, &[])), None);
    }

    #[test]
    fn encodes_only_what_a_dictation_holds() {
        assert_eq!(audio_context(4 * 16_000, false), Some(300));
        assert_eq!(audio_context(MIN_SAMPLES, false), Some(200));
        assert_eq!(audio_context(4 * 16_000, true), None);
        assert_eq!(audio_context(28 * 16_000, false), None);
    }

    #[test]
    fn drops_non_speech_markers() {
        assert!(is_annotation("[BLANK_AUDIO]"));
        assert!(is_annotation("(music)"));
        assert!(!is_annotation("Priya sent the report."));
    }
}
