// FilePath: crates/sv-mobile/src/local.rs
//! An iPhone dictation transcribed on the device by Apple Speech while offline (MB-4). It gets
//! the deterministic rules only: the formatting pass needs Groq, which is unreachable offline.

use std::path::Path;

use sv_domain::models::APPLE_SPEECH;
use sv_domain::{HistoryStatus, NewHistory};
use sv_storage::Db;

use crate::pipeline::{remove_audio, vocabulary, with_separator};
use crate::{Dictation, Recording};

/// What Apple Speech heard in a recording.
#[derive(Debug, Clone)]
pub struct LocalTranscript {
    pub text: String,
    pub language: Option<String>,
    pub transcribe_ms: i64,
}

/// Formats `heard` with the rules, saves a history row and deletes the recording.
pub async fn dictate_local(db: &Db, heard: LocalTranscript, recording: Recording) -> Dictation {
    remove_audio(&recording.wav_path).await;
    if heard.text.trim().is_empty() {
        return Dictation::NoSpeech;
    }
    let settings = match db.settings().await {
        Ok(settings) => settings,
        Err(error) => {
            return Dictation::Failed {
                id: None,
                message: error.to_string(),
            }
        }
    };
    let vocabulary = vocabulary(db).await;
    let formatted = sv_text::format(&heard.text, &vocabulary, settings.style);
    let row = NewHistory {
        created_at: chrono::Utc::now().timestamp_millis(),
        status: HistoryStatus::Pasted,
        raw_text: heard.text,
        final_text: formatted.text.clone(),
        source_text: None,
        error: None,
        model: APPLE_SPEECH.to_owned(),
        format_model: None,
        language: heard.language,
        style: settings.style,
        audio_ms: recording.duration_ms,
        latency_ms: heard.transcribe_ms,
        transcribe_ms: Some(heard.transcribe_ms),
        format_ms: None,
        dictionary_fixes: i64::from(formatted.rule_hits),
        app_name: recording.app_name,
        bundle_id: recording.app_id,
        audio_path: None,
    };
    let id = match db.insert_history(row).await {
        Ok(entry) => entry.id,
        Err(error) => {
            // The text is still worth inserting; only the history row is missing.
            tracing::error!(%error, "could not save the dictation to history");
            0
        }
    };
    Dictation::Text {
        id,
        text: with_separator(&formatted.text),
        unformatted: false,
    }
}

/// Deletes a recording nobody will transcribe (the keyboard cancelled it).
pub async fn discard(wav_path: &Path) {
    remove_audio(wav_path).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_offline_transcript_gets_the_rules_a_row_and_no_audio() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        let wav_path = dir.path().join("session.wav");
        std::fs::write(&wav_path, [0_u8; 64]).unwrap();
        let heard = LocalTranscript {
            text: "send the notes to priya".to_owned(),
            language: Some("en".to_owned()),
            transcribe_ms: 420,
        };
        let recording = Recording {
            wav_path: wav_path.clone(),
            duration_ms: 2_000,
            app_name: None,
            app_id: None,
        };

        let Dictation::Text { id, text, .. } = dictate_local(&db, heard, recording).await else {
            panic!("expected text");
        };

        assert!(
            text.ends_with(' '),
            "dictations end in whitespace: {text:?}"
        );
        assert!(
            !text.to_lowercase().starts_with("um"),
            "fillers are removed: {text:?}"
        );
        assert!(!wav_path.exists());
        let entry = db.history_entry(id).await.unwrap().unwrap();
        assert_eq!(entry.model, APPLE_SPEECH);
        assert_eq!(entry.format_model_name, None);
    }

    #[tokio::test]
    async fn silence_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        let heard = LocalTranscript {
            text: "  ".to_owned(),
            language: None,
            transcribe_ms: 10,
        };
        let recording = Recording {
            wav_path: dir.path().join("none.wav"),
            duration_ms: 500,
            app_name: None,
            app_id: None,
        };

        assert_eq!(
            dictate_local(&db, heard, recording).await,
            Dictation::NoSpeech
        );
        assert!(db.list_history(None, 10, None).await.unwrap().is_empty());
    }
}
