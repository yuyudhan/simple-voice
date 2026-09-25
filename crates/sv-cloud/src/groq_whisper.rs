// FilePath: crates/sv-cloud/src/groq_whisper.rs
//! Groq Whisper transcription. Whisper auto-detects the language; a detection outside the
//! user's languages (Hindi is often heard as Urdu, Punjabi or Nepali) is re-run once pinned to
//! the fallback language.

use std::time::Duration;

use bytes::Bytes;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use sv_domain::{AppError, AppResult};

const TRANSCRIPTION_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const MODELS_URL: &str = "https://api.groq.com/openai/v1/models";
const WHISPER_MODEL: &str = "whisper-large-v3-turbo";
const TRANSCRIPTION_TIMEOUT: Duration = Duration::from_secs(60);
const VERIFY_TIMEOUT: Duration = Duration::from_secs(15);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub text: String,
    /// ISO 639-1 code of the language the text is in, when known.
    pub language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WhisperResponse {
    #[serde(default)]
    text: String,
    #[serde(default)]
    language: Option<String>,
}

/// Transcribes a WAV recording. `languages` are the allowed ISO 639-1 codes; `prompt` biases
/// spelling (see `sv_text::Vocabulary::prompt`).
pub async fn groq_transcribe(
    client: &reqwest::Client,
    key: &str,
    wav: Vec<u8>,
    prompt: &str,
    languages: &[String],
    fallback_language: &str,
) -> AppResult<Transcript> {
    // `Bytes` lets the re-run reuse the upload buffer without copying it.
    let wav = Bytes::from(wav);
    let detected = request(client, key, wav.clone(), prompt, None).await?;
    let Some(name) = detected.language.as_deref() else {
        return Ok(Transcript {
            text: detected.text.trim().to_owned(),
            language: None,
        });
    };

    let code = language_code(name);
    let allowed = code.is_some_and(|code| languages.iter().any(|l| l.eq_ignore_ascii_case(code)));
    let fallback = fallback_language.trim();
    if allowed || fallback.is_empty() {
        return Ok(Transcript {
            text: detected.text.trim().to_owned(),
            language: code.map(str::to_owned),
        });
    }

    tracing::info!(
        detected = name,
        fallback,
        "language outside the allowed set; re-running"
    );
    let pinned = request(client, key, wav, prompt, Some(fallback)).await?;
    Ok(Transcript {
        text: pinned.text.trim().to_owned(),
        language: Some(fallback.to_owned()),
    })
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
    key: &str,
    wav: Bytes,
    prompt: &str,
    language: Option<&str>,
) -> AppResult<WhisperResponse> {
    let length = wav.len() as u64;
    let file = Part::stream_with_length(wav, length)
        .file_name("recording.wav")
        .mime_str("audio/wav")
        .map_err(AppError::other)?;
    let mut form = Form::new()
        .text("model", WHISPER_MODEL)
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
        .bearer_auth(key.trim())
        .multipart(form)
        .timeout(TRANSCRIPTION_TIMEOUT)
        .send()
        .await
        .map_err(|error| transport_error(&error, TRANSCRIPTION_TIMEOUT))?;
    let status = response.status().as_u16();
    let body = response
        .bytes()
        .await
        .map_err(|error| transport_error(&error, TRANSCRIPTION_TIMEOUT))?;
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
}
