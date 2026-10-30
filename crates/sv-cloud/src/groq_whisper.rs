// FilePath: crates/sv-cloud/src/groq_whisper.rs
//! Groq Whisper transcription. Whisper auto-detects the language; a detection outside the
//! user's languages (Hindi is often heard as Urdu, Punjabi or Nepali) is re-run once pinned to
//! the fallback language.

use std::fmt;
use std::time::Duration;

use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde::de::IgnoredAny;
use serde::Deserialize;
use sv_domain::{AppError, AppResult, TranscriptQuality};

const TRANSCRIPTION_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const MODELS_URL: &str = "https://api.groq.com/openai/v1/models";
const VERIFY_TIMEOUT: Duration = Duration::from_secs(15);
/// Groq's upload limit on the free tier; a longer recording is refused before it is sent.
const MAX_UPLOAD_BYTES: usize = 25 * 1024 * 1024;
/// The time any request gets, before the upload's share.
const BASE_TIMEOUT: Duration = Duration::from_secs(10);
/// Upload bytes per extra second of time (16 kHz mono 16-bit WAV is 32 kB per second of audio).
const BYTES_PER_SECOND: usize = 512 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(90);

/// Which Groq Whisper model transcribes, with the user's key.
pub struct GroqWhisper<'a> {
    pub key: &'a str,
    /// One of Groq's Whisper model ids, such as `whisper-large-v3-turbo`.
    pub model: &'a str,
}

// The key never reaches logs.
impl fmt::Debug for GroqWhisper<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroqWhisper")
            .field("key", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

/// How long one transcription request of `wav_bytes` may take: long enough to upload the
/// recording on a slow connection, short enough that a stalled request falls back to the
/// on-device model while the user still waits for the text.
pub fn upload_timeout(wav_bytes: usize) -> Duration {
    let upload =
        Duration::from_secs(u64::try_from(wav_bytes / BYTES_PER_SECOND).unwrap_or(u64::MAX));
    BASE_TIMEOUT.saturating_add(upload).min(MAX_TIMEOUT)
}

/// Whisper language names (as Groq's `verbose_json` reports them, lowercased) → ISO 639-1.
const LANGUAGE_CODES: &[(&str, &str)] = &[
    ("english", "en"),
    ("hindi", "hi"),
    ("urdu", "ur"),
    ("punjabi", "pa"),
    ("nepali", "ne"),
    ("marathi", "mr"),
    ("bengali", "bn"),
    ("gujarati", "gu"),
    ("tamil", "ta"),
    ("telugu", "te"),
    ("kannada", "kn"),
    ("malayalam", "ml"),
    ("sanskrit", "sa"),
    ("sindhi", "sd"),
    ("assamese", "as"),
    ("sinhala", "si"),
    ("chinese", "zh"),
    ("spanish", "es"),
    ("french", "fr"),
    ("german", "de"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("dutch", "nl"),
    ("russian", "ru"),
    ("japanese", "ja"),
    ("korean", "ko"),
    ("arabic", "ar"),
    ("persian", "fa"),
    ("turkish", "tr"),
    ("polish", "pl"),
    ("ukrainian", "uk"),
    ("vietnamese", "vi"),
    ("indonesian", "id"),
    ("malay", "ms"),
    ("thai", "th"),
    ("swedish", "sv"),
    ("norwegian", "no"),
    ("danish", "da"),
    ("finnish", "fi"),
    ("greek", "el"),
    ("hebrew", "he"),
    ("czech", "cs"),
    ("romanian", "ro"),
    ("hungarian", "hu"),
    ("catalan", "ca"),
    ("tagalog", "tl"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    /// ISO 639-1 code of the language the text is in, when known.
    pub language: Option<String>,
    /// Aggregated over the segments of the response the text came from.
    pub quality: TranscriptQuality,
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    segments: Vec<Segment>,
}

/// One `verbose_json` segment; only the fields the quality gate reads.
#[derive(Debug, Deserialize)]
struct Segment {
    #[serde(default)]
    avg_logprob: Option<f32>,
    #[serde(default)]
    compression_ratio: Option<f32>,
    #[serde(default)]
    no_speech_prob: Option<f32>,
    /// Only the count matters: it weights the segment's means.
    #[serde(default)]
    tokens: Vec<IgnoredAny>,
}

impl WhisperResponse {
    fn into_transcript(self, language: Option<String>) -> Transcript {
        Transcript {
            text: self.text.trim().to_owned(),
            language,
            quality: segment_quality(&self.segments),
        }
    }
}

/// Aggregates like the engine helper does for local Whisper: log probability and no-speech
/// probability are means weighted by each segment's token count (1 for a segment without
/// tokens), the compression ratio is the worst (highest) segment's.
fn segment_quality(segments: &[Segment]) -> TranscriptQuality {
    let weighted = |value: fn(&Segment) -> Option<f32>| {
        let (sum, weight) = segments
            .iter()
            .filter_map(|segment| {
                let weight = segment.tokens.len().max(1) as f64;
                value(segment).map(|value| (f64::from(value) * weight, weight))
            })
            .fold((0.0, 0.0), |(sum, total), (value, weight)| {
                (sum + value, total + weight)
            });
        (weight > 0.0).then(|| (sum / weight) as f32)
    };
    TranscriptQuality {
        avg_logprob: weighted(|segment| segment.avg_logprob),
        compression_ratio: segments
            .iter()
            .filter_map(|segment| segment.compression_ratio)
            .reduce(f32::max),
        no_speech_prob: weighted(|segment| segment.no_speech_prob),
        confidence: None,
    }
}

/// Transcribes a WAV recording. `languages` are the allowed ISO 639-1 codes; `prompt` biases
/// spelling (see `sv_text::whisper_prompt`). Each request gets [`upload_timeout`].
pub async fn groq_transcribe(
    client: &reqwest::Client,
    whisper: &GroqWhisper<'_>,
    wav: Vec<u8>,
    prompt: &str,
    languages: &[String],
    fallback_language: &str,
) -> AppResult<Transcript> {
    if wav.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::network(format!(
            "The recording is larger than Groq's {} MB upload limit",
            MAX_UPLOAD_BYTES / (1024 * 1024)
        )));
    }
    let limit = upload_timeout(wav.len());
    // `Bytes` lets the re-run reuse the upload buffer without copying it.
    let wav = Bytes::from(wav);
    let detected = request(client, whisper, wav.clone(), prompt, None, limit).await?;
    let Some(name) = detected.language.clone() else {
        return Ok(detected.into_transcript(None));
    };

    let code = language_code(&name);
    let allowed = code.is_some_and(|code| languages.iter().any(|l| l.eq_ignore_ascii_case(code)));
    let fallback = fallback_language.trim();
    if allowed || fallback.is_empty() {
        return Ok(detected.into_transcript(code.map(str::to_owned)));
    }

    tracing::info!(
        detected = name.as_str(),
        fallback,
        "language outside the allowed set; re-running"
    );
    // The pinned re-run's text is what gets used, so its segments are the ones that count.
    let pinned = request(client, whisper, wav, prompt, Some(fallback), limit).await?;
    Ok(pinned.into_transcript(Some(fallback.to_owned())))
}

/// Checks a key against Groq's model list, so Settings can confirm it before saving.
pub async fn groq_verify_key(client: &reqwest::Client, key: &str) -> AppResult<()> {
    let response = client
        .get(MODELS_URL)
        .bearer_auth(key.trim())
        .timeout(VERIFY_TIMEOUT)
        .send()
        .await
        .map_err(|error| transport_error(&error, VERIFY_TIMEOUT))?;
    let status = response.status().as_u16();
    if status == 401 {
        return Err(AppError::invalid("Groq rejected this API key"));
    }
    if status != 200 {
        let body = response.bytes().await.unwrap_or_default();
        return Err(http_error(status, &body));
    }
    Ok(())
}

async fn request(
    client: &reqwest::Client,
    whisper: &GroqWhisper<'_>,
    wav: Bytes,
    prompt: &str,
    language: Option<&str>,
    limit: Duration,
) -> AppResult<WhisperResponse> {
    let length = wav.len() as u64;
    let file = Part::stream_with_length(wav, length)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(AppError::other)?;
    let mut form = Form::new()
        .text("model", whisper.model.to_owned())
        .text("response_format", "verbose_json")
        .part("file", file);
    if !prompt.is_empty() {
        form = form.text("prompt", prompt.to_owned());
    }
    if let Some(language) = language {
        form = form.text("language", language.to_owned());
    }

    let response = client
        .post(TRANSCRIPTION_URL)
        .bearer_auth(whisper.key.trim())
        .multipart(form)
        .timeout(limit)
        .send()
        .await
        .map_err(|error| transport_error(&error, limit))?;
    let status = response.status().as_u16();
    let body = response
        .bytes()
        .await
        .map_err(|error| transport_error(&error, limit))?;
    if status != 200 {
        return Err(http_error(status, &body));
    }
    serde_json::from_slice(&body)
        .map_err(|_| AppError::network("Groq returned a transcription it could not read"))
}

/// ISO 639-1 code for a Whisper language name; codes pass through unchanged.
fn language_code(name: &str) -> Option<&'static str> {
    let name = name.trim();
    LANGUAGE_CODES.iter().find_map(|&(language, code)| {
        (language.eq_ignore_ascii_case(name) || code.eq_ignore_ascii_case(name)).then_some(code)
    })
}

fn transport_error(error: &reqwest::Error, limit: Duration) -> AppError {
    if error.is_timeout() {
        AppError::network(format!("Groq did not answer within {} s", limit.as_secs()))
    } else {
        AppError::network(format!("Could not reach Groq: {error}"))
    }
}

fn http_error(status: u16, body: &[u8]) -> AppError {
    let message = crate::api_error_message(body).unwrap_or_else(|| format!("Groq HTTP {status}"));
    AppError::Network(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_whisper_language_names_to_iso_codes() {
        assert_eq!(language_code("Hindi"), Some("hi"));
        assert_eq!(language_code("english"), Some("en"));
        assert_eq!(language_code("Urdu"), Some("ur"));
        assert_eq!(language_code("Punjabi"), Some("pa"));
        assert_eq!(language_code("NEPALI"), Some("ne"));
        assert_eq!(language_code("Marathi"), Some("mr"));
        assert_eq!(language_code("Bengali"), Some("bn"));
        assert_eq!(language_code("hi"), Some("hi"));
        assert_eq!(language_code("klingon"), None);
    }

    #[test]
    fn every_language_name_and_code_is_unique() {
        for (index, (name, code)) in LANGUAGE_CODES.iter().enumerate() {
            let mut rest = LANGUAGE_CODES.iter().skip(index + 1);
            assert!(rest.all(|(n, c)| n != name && c != code), "{name}/{code}");
            assert!(
                LANGUAGE_CODES.iter().all(|(n, _)| n != code),
                "{code} is also a name"
            );
        }
    }

    #[test]
    fn upload_time_grows_with_the_recording_and_is_capped() {
        assert_eq!(upload_timeout(0), BASE_TIMEOUT);
        // A minute of 16 kHz mono audio (~1.9 MB) gets a few extra seconds.
        let minute = upload_timeout(60 * 32_000);
        assert!(minute > BASE_TIMEOUT && minute < Duration::from_secs(15));
        assert_eq!(upload_timeout(MAX_UPLOAD_BYTES * 4), MAX_TIMEOUT);
    }

    #[test]
    fn non_200_errors_prefer_groqs_message() {
        let body = br#"{"error":{"message":"file must be one of flac, mp3, wav"}}"#;
        assert_eq!(
            http_error(400, body),
            AppError::Network("file must be one of flac, mp3, wav".into())
        );
        assert_eq!(
            http_error(503, b"upstream down"),
            AppError::Network("Groq HTTP 503".into())
        );
    }

    /// Shaped like Groq's `verbose_json` response for a two-segment Hinglish clip.
    const VERBOSE_JSON: &str = r#"{
        "task": "transcribe", "language": "Hindi", "duration": 6.2,
        "text": " Kal meeting hai. Please send the deck.",
        "segments": [
            {"id": 0, "seek": 0, "start": 0.0, "end": 2.4, "text": " Kal meeting hai.",
             "tokens": [50365, 12489, 3440, 10737, 13, 50485], "temperature": 0.0,
             "avg_logprob": -0.5, "compression_ratio": 0.9, "no_speech_prob": 0.02},
            {"id": 1, "seek": 0, "start": 2.4, "end": 6.2, "text": " Please send the deck.",
             "tokens": [50485, 2555], "temperature": 0.0,
             "avg_logprob": -1.3, "compression_ratio": 2.6, "no_speech_prob": 0.1}
        ],
        "x_groq": {"id": "req_01"}
    }"#;

    #[test]
    fn quality_weights_means_by_tokens_and_keeps_the_worst_compression() {
        let response: WhisperResponse = serde_json::from_str(VERBOSE_JSON).unwrap();
        let transcript = response.into_transcript(Some("hi".to_owned()));
        assert_eq!(transcript.text, "Kal meeting hai. Please send the deck.");
        let quality = transcript.quality;
        // (6 × -0.5 + 2 × -1.3) / 8 and (6 × 0.02 + 2 × 0.1) / 8.
        assert!((quality.avg_logprob.unwrap() - -0.7).abs() < 1e-5);
        assert!((quality.no_speech_prob.unwrap() - 0.04).abs() < 1e-5);
        assert_eq!(quality.compression_ratio, Some(2.6));
        assert_eq!(quality.confidence, None);
    }

    #[test]
    fn segments_without_tokens_weigh_one_and_missing_fields_are_skipped() {
        let json = r#"{"text": "ok", "segments": [
            {"avg_logprob": -0.2, "compression_ratio": 1.1},
            {"avg_logprob": -0.8, "tokens": []},
            {"tokens": [1, 2, 3]}
        ]}"#;
        let response: WhisperResponse = serde_json::from_str(json).unwrap();
        let quality = response.into_transcript(None).quality;
        assert!((quality.avg_logprob.unwrap() - -0.5).abs() < 1e-6);
        assert_eq!(quality.no_speech_prob, None);
        assert_eq!(quality.compression_ratio, Some(1.1));
    }

    #[test]
    fn a_response_without_segments_has_no_quality() {
        let response: WhisperResponse = serde_json::from_str(r#"{"text": " hi "}"#).unwrap();
        let transcript = response.into_transcript(None);
        assert_eq!(transcript.text, "hi");
        assert_eq!(transcript.quality, TranscriptQuality::default());
    }
}
