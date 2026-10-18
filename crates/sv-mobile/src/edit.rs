// FilePath: crates/sv-mobile/src/edit.rs
//! Edit mode on the phones (MB-6): the keyboard hands over the selected text and a recording of
//! how to change it. The instruction is transcribed with Groq Whisper and gets the rules, then
//! the Groq chat model applies it with the desktop's prompt and checks (`sv_text::edit_prompt`,
//! `accept_edit`). Phones have one AI provider and no formatting switch, so an edit always uses
//! Groq. Unlike the desktop, a failed edit keeps its audio: the keyboard still holds the
//! selection, so it can retry in place (MB-5). History never retries an edit itself.

use std::path::Path;
use std::time::Instant;

use serde::Serialize;
use sv_cloud::ChatEndpoint;
use sv_domain::models::GROQ_WHISPER;
use sv_domain::{HistoryStatus, NewHistory, Settings};
use sv_storage::Db;
use sv_text::EDIT_MAX_CHARS;

use crate::pipeline::{elapsed_ms, groq_key, read_wav, remove_audio, transcribe, vocabulary};
use crate::status::NO_GROQ_KEY;
use crate::Recording;

const NO_SELECTION: &str = "Select the text to edit first";

/// One edit: the selection it rewrites and the recording of the spoken instruction.
#[derive(Debug, Clone)]
pub struct EditRequest {
    pub selection: String,
    pub recording: Recording,
    /// The history row of the failed attempt this one retries; it is updated, not duplicated.
    pub retry_of: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Edit {
    /// The replacement for the selection, with the selection's surrounding whitespace. `id` is
    /// its history row, for [`Db::mark_history_not_pasted`] if it never lands.
    Text { id: i64, text: String },
    /// Nothing was said; nothing is kept and the selection stays.
    NoSpeech,
    /// The selection stays. With an `id` the audio is kept and the same selection can be retried;
    /// without one, retrying cannot help and the audio is deleted.
    Failed { id: Option<i64>, message: String },
}

/// What the Groq chat model made of the selection.
struct Rewrite {
    text: String,
    model: String,
    ms: i64,
}

pub async fn edit(db: &Db, http: &reqwest::Client, request: EditRequest) -> Edit {
    let EditRequest {
        selection,
        recording,
        retry_of,
    } = request;
    let started = Instant::now();
    let wav_path = recording.wav_path.clone();
    if let Err(message) = check_selection(&selection) {
        remove_audio(&wav_path).await;
        return Edit::Failed { id: None, message };
    }
    let settings = match db.settings().await {
        Ok(settings) => settings,
        Err(error) => {
            remove_audio(&wav_path).await;
            return Edit::Failed {
                id: None,
                message: error.to_string(),
            };
        }
    };
    let created_at = match retry_of {
        Some(id) => db.history_entry(id).await.ok().flatten(),
        None => None,
    }
    .map_or_else(
        || chrono::Utc::now().timestamp_millis(),
        |entry| entry.created_at,
    );
    let mut row = NewHistory {
        created_at,
        status: HistoryStatus::Failed,
        raw_text: String::new(),
        final_text: String::new(),
        source_text: Some(selection.clone()),
        error: None,
        model: GROQ_WHISPER.to_owned(),
        format_model: None,
        language: None,
        style: settings.style,
        audio_ms: recording.duration_ms,
        latency_ms: 0,
        transcribe_ms: None,
        format_ms: None,
        dictionary_fixes: 0,
        app_name: recording.app_name,
        bundle_id: recording.app_id,
        // The keyboard owns a failed edit's audio; a row with a path would offer Retry in
        // history, which no longer has the selection to apply it to.
        audio_path: None,
        first_model: None,
        retry_reason: None,
    };

    let vocabulary = vocabulary(db).await;
    let heard = match read_wav(&wav_path).await {
        Ok(wav) => transcribe(db, http, &settings, &vocabulary, wav).await,
        Err(error) => Err(error),
    };
    let heard = match heard {
        Ok(heard) if sv_text::only_hesitations(&heard.text) => {
            remove_audio(&wav_path).await;
            return Edit::NoSpeech;
        }
        Ok(heard) => heard,
        Err(error) => {
            let outcome = Err(error.to_string());
            return conclude(db, row, outcome, retry_of, &wav_path, started).await;
        }
    };
    let instruction = sv_text::format(&heard.text, &vocabulary, settings.style);
    row.raw_text = heard.text;
    row.language = heard.language;
    row.transcribe_ms = Some(heard.ms);
    row.dictionary_fixes = i64::from(instruction.rule_hits);

    let outcome = rewrite(
        db,
        http,
        &settings,
        &vocabulary.terms,
        &selection,
        &instruction.text,
    )
    .await
    .map_err(|reason| format!("Edit failed: {reason}"));
    conclude(db, row, outcome, retry_of, &wav_path, started).await
}

/// The desktop's limits (ED-3): something to edit, and at most [`EDIT_MAX_CHARS`] of it.
fn check_selection(selection: &str) -> Result<(), String> {
    if selection.chars().count() > EDIT_MAX_CHARS {
        return Err(format!(
            "Select at most {EDIT_MAX_CHARS} characters to edit"
        ));
    }
    if selection.trim().is_empty() {
        return Err(NO_SELECTION.to_owned());
    }
    Ok(())
}

async fn rewrite(
    db: &Db,
    http: &reqwest::Client,
    settings: &Settings,
    terms: &[String],
    selection: &str,
    instruction: &str,
) -> Result<Rewrite, String> {
    let key = match groq_key(db).await {
        Ok(Some(key)) => key,
        Ok(None) => return Err(NO_GROQ_KEY.to_owned()),
        Err(error) => return Err(error.to_string()),
    };
    let prompt = sv_text::edit_prompt(selection, instruction, terms, &settings.languages);
    let endpoint = ChatEndpoint {
        url: sv_cloud::GROQ_CHAT_URL.to_owned(),
        key: Some(&key),
        model: &settings.groq_formatting_model,
        groq_no_reasoning: true,
    };
    let started = Instant::now();
    let limit = sv_text::edit_timeout(selection);
    let max_tokens = sv_text::edit_max_tokens(selection);
    let reply = sv_cloud::chat_completion(http, &endpoint, &prompt, max_tokens, limit).await?;
    let text = sv_text::accept_edit(selection, &reply.text, reply.finished)?;
    Ok(Rewrite {
        text,
        model: settings.groq_formatting_model.clone(),
        ms: elapsed_ms(started),
    })
}

/// Saves the edit's row (a retry updates the failed attempt's row) and settles its audio: kept
/// only for a failure the keyboard can retry, which needs a saved row.
async fn conclude(
    db: &Db,
    mut row: NewHistory,
    outcome: Result<Rewrite, String>,
    retry_of: Option<i64>,
    wav_path: &Path,
    started: Instant,
) -> Edit {
    row.latency_ms = elapsed_ms(started);
    let edited = match outcome {
        Ok(rewrite) => {
            row.status = HistoryStatus::Pasted;
            row.format_model = Some(rewrite.model);
            row.format_ms = Some(rewrite.ms);
            row.final_text.clone_from(&rewrite.text);
            Ok(rewrite.text)
        }
        Err(message) => {
            row.error = Some(message.clone());
            Err(message)
        }
    };
    let saved = match retry_of {
        Some(id) => db.update_history(id, row).await,
        None => db.insert_history(row).await,
    };
    let id = match saved {
        Ok(entry) => Some(entry.id),
        Err(error) => {
            tracing::error!(%error, "could not save the edit to history");
            None
        }
    };
    match edited {
        Ok(text) => {
            remove_audio(wav_path).await;
            // The text is still worth delivering; only the history row is missing.
            Edit::Text {
                id: id.unwrap_or(0),
                text,
            }
        }
        Err(message) => {
            if id.is_none() {
                remove_audio(wav_path).await;
            }
            Edit::Failed { id, message }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    const SELECTION: &str = "hey team, the build is broken again ";

    fn write_wav(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        // A WAV header and one second of silence: enough for the pipeline to try it.
        std::fs::write(&path, vec![0_u8; 44 + 32_000]).unwrap();
        path
    }

    fn request(wav_path: PathBuf, selection: &str, retry_of: Option<i64>) -> EditRequest {
        EditRequest {
            selection: selection.to_owned(),
            recording: Recording {
                wav_path,
                duration_ms: 1_000,
                app_name: Some("Messages".to_owned()),
                app_id: Some("com.google.android.apps.messaging".to_owned()),
            },
            retry_of,
        }
    }

    async fn open() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        (dir, db)
    }

    async fn history(db: &Db) -> Vec<sv_domain::HistoryEntry> {
        db.list_history(None, 50, None).await.unwrap()
    }

    #[tokio::test]
    async fn a_selection_over_the_limit_is_never_sent_and_leaves_nothing_behind() {
        let (dir, db) = open().await;
        let wav_path = write_wav(dir.path(), "edit-long.wav");
        let long = "a".repeat(EDIT_MAX_CHARS + 1);

        let outcome = edit(&db, &http_client(), request(wav_path.clone(), &long, None)).await;
        assert_eq!(
            outcome,
            Edit::Failed {
                id: None,
                message: "Select at most 4000 characters to edit".to_owned(),
            }
        );
        assert!(
            !wav_path.exists(),
            "a retry could not help, so the audio goes"
        );
        assert!(history(&db).await.is_empty());

        let blank = request(write_wav(dir.path(), "b.wav"), " \n", None);
        let blank = edit(&db, &http_client(), blank).await;
        assert!(matches!(blank, Edit::Failed { id: None, message } if message == NO_SELECTION));
    }

    #[tokio::test]
    async fn a_failed_edit_keeps_its_audio_for_the_keyboard_but_not_for_history() {
        let (dir, db) = open().await;
        if db.groq_api_key().await.unwrap().is_some() {
            // GROQ_API_KEY in the environment would reach the network.
            return;
        }
        let wav_path = write_wav(dir.path(), "edit-1.wav");

        let outcome = edit(
            &db,
            &http_client(),
            request(wav_path.clone(), SELECTION, None),
        )
        .await;
        let Edit::Failed {
            id: Some(id),
            message,
        } = outcome
        else {
            panic!("expected a saved failure, got {outcome:?}");
        };
        assert_eq!(message, NO_GROQ_KEY);
        assert!(wav_path.exists(), "the keyboard retries from this audio");
        let entry = db.history_entry(id).await.unwrap().unwrap();
        assert_eq!(entry.status, HistoryStatus::Failed);
        assert_eq!(entry.source_text.as_deref(), Some(SELECTION));
        assert!(!entry.can_retry, "history no longer holds the selection");

        // A retry that fails again updates the same row and still keeps the audio.
        let again = request(wav_path.clone(), SELECTION, Some(id));
        let again = edit(&db, &http_client(), again).await;
        assert!(matches!(again, Edit::Failed { id: Some(same), .. } if same == id));
        assert_eq!(history(&db).await.len(), 1);
        assert!(wav_path.exists());
    }

    #[tokio::test]
    async fn a_retried_edit_replaces_its_failed_row_and_deletes_the_audio() {
        let (dir, db) = open().await;
        let wav_path = write_wav(dir.path(), "edit-2.wav");
        let failed = conclude(
            &db,
            row(),
            Err("Edit failed: Groq timed out".to_owned()),
            None,
            &wav_path,
            Instant::now(),
        )
        .await;
        let Edit::Failed { id: Some(id), .. } = failed else {
            panic!("expected a saved failure, got {failed:?}");
        };
        assert!(wav_path.exists());

        let rewrite = Rewrite {
            text: "Hello team, the build is failing again. ".to_owned(),
            model: "openai/gpt-oss-20b".to_owned(),
            ms: 420,
        };
        let done = conclude(&db, row(), Ok(rewrite), Some(id), &wav_path, Instant::now()).await;
        assert_eq!(
            done,
            Edit::Text {
                id,
                text: "Hello team, the build is failing again. ".to_owned(),
            }
        );
        assert!(!wav_path.exists());
        let rows = history(&db).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, HistoryStatus::Pasted);
        assert_eq!(rows[0].source_text.as_deref(), Some(SELECTION));
        assert_eq!(rows[0].text, "Hello team, the build is failing again. ");
        assert_eq!(rows[0].error, None);
        assert_eq!(rows[0].format_ms, Some(420));
    }

    fn row() -> NewHistory {
        NewHistory {
            created_at: 1,
            status: HistoryStatus::Failed,
            raw_text: "make it formal".to_owned(),
            final_text: String::new(),
            source_text: Some(SELECTION.to_owned()),
            error: None,
            model: GROQ_WHISPER.to_owned(),
            format_model: None,
            language: Some("en".to_owned()),
            style: sv_domain::Style::default(),
            audio_ms: 1_000,
            latency_ms: 0,
            transcribe_ms: Some(300),
            format_ms: None,
            dictionary_fixes: 0,
            app_name: None,
            bundle_id: None,
            audio_path: None,
            first_model: None,
            retry_reason: None,
        }
    }

    fn http_client() -> reqwest::Client {
        crate::http_client().unwrap()
    }
}
