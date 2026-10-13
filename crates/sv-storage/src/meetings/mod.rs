// FilePath: crates/sv-storage/src/meetings/mod.rs
//! Personal meetings: the recording's row, its timestamped transcript split by speaker, and the
//! summary, questions and translations the on-device language model wrote about it. Deleting a
//! meeting cascades to everything it owns; its audio folder is the caller's to remove.

use sqlx::{Executor, Sqlite};
use sv_domain::{
    ActionItem, AppError, AppResult, ChatMessage, ChatRole, Citation, Meeting, MeetingDetail,
    MeetingStatus, MeetingSummary, Segment, Speaker, TranslatedSegment, Translation,
};

use crate::db::Db;

mod transcript;

const MAX_TITLE_CHARS: usize = 200;
const MAX_SPEAKER_NAME_CHARS: usize = 80;
const MAX_SEGMENT_CHARS: usize = 10_000;
/// Recorded on meetings a quit or crash interrupted; their audio stays for `retry_meeting`.
pub(crate) const INTERRUPTED: &str = "Simple Voice quit before this meeting finished recording \
or processing. The audio recorded until then is kept: choose Retry to process it.";

/// One transcript segment to store, its speaker an index into the meeting's speakers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    /// Diarized speaker index; any numbering works, labels follow first appearance.
    pub speaker: Option<usize>,
    pub text: String,
}

#[derive(Debug)]
struct MeetingRow {
    id: i64,
    title: String,
    started_at: i64,
    ended_at: Option<i64>,
    duration_ms: i64,
    status: String,
    microphone: bool,
    system_audio: bool,
    language: Option<String>,
    error: Option<String>,
    segment_count: i64,
    speaker_count: i64,
}

impl MeetingRow {
    fn into_meeting(self) -> AppResult<Meeting> {
        let status = MeetingStatus::parse(&self.status).ok_or_else(|| {
            AppError::Database(format!(
                "Meeting {} has unknown status “{}”",
                self.id, self.status
            ))
        })?;
        Ok(Meeting {
            id: self.id,
            title: self.title,
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            status,
            microphone: self.microphone,
            system_audio: self.system_audio,
            language: self.language,
            error: self.error,
            segment_count: self.segment_count,
            speaker_count: self.speaker_count,
        })
    }
}

impl Db {
    /// Starts a meeting row in `recording` status.
    pub async fn create_meeting(
        &self,
        title: &str,
        started_at: i64,
        microphone: bool,
        system_audio: bool,
        audio_dir: &str,
    ) -> AppResult<Meeting> {
        let title = clean_title(title)?;
        let status = MeetingStatus::Recording.as_str();
        let inner = self.read().await;
        let id = sqlx::query!(
            "INSERT INTO meetings (title, started_at, status, microphone, system_audio, audio_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            title,
            started_at,
            status,
            microphone,
            system_audio,
            audio_dir
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?
        .last_insert_rowid();
        stored_meeting(&inner.pool, id).await
    }

    /// Every meeting, newest first.
    pub async fn meetings(&self) -> AppResult<Vec<Meeting>> {
        let inner = self.read().await;
        let rows = sqlx::query_as!(
            MeetingRow,
            r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                      m.started_at AS "started_at!: i64", m.ended_at AS "ended_at?: i64",
                      m.duration_ms AS "duration_ms!: i64", m.status AS "status!: String",
                      m.microphone AS "microphone!: bool", m.system_audio AS "system_audio!: bool",
                      m.language AS "language?: String", m.error AS "error?: String",
                      (SELECT COUNT(*) FROM meeting_segments s WHERE s.meeting_id = m.id)
                          AS "segment_count!: i64",
                      (SELECT COUNT(*) FROM meeting_speakers p WHERE p.meeting_id = m.id)
                          AS "speaker_count!: i64"
               FROM meetings m ORDER BY m.started_at DESC, m.id DESC"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        rows.into_iter().map(MeetingRow::into_meeting).collect()
    }

    pub async fn meeting(&self, id: i64) -> AppResult<Meeting> {
        let inner = self.read().await;
        stored_meeting(&inner.pool, id).await
    }

    /// The folder holding the meeting's WAV files.
    pub async fn meeting_audio_dir(&self, id: i64) -> AppResult<String> {
        let inner = self.read().await;
        sqlx::query_scalar!(
            r#"SELECT audio_dir AS "audio_dir!: String" FROM meetings WHERE id = ?1"#,
            id
        )
        .fetch_optional(&inner.pool)
        .await
        .map_err(AppError::database)?
        .ok_or_else(|| not_found(id))
    }

    /// The meeting with its speakers, transcript in time order, summary, chat and translations.
    pub async fn meeting_detail(&self, id: i64) -> AppResult<MeetingDetail> {
        let inner = self.read().await;
        let meeting = stored_meeting(&inner.pool, id).await?;
        let speakers = sqlx::query_as!(
            Speaker,
            r#"SELECT id AS "id!: i64", label AS "label!: String", name AS "name?: String"
               FROM meeting_speakers WHERE meeting_id = ?1 ORDER BY id"#,
            id
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        let segments = sqlx::query_as!(
            Segment,
            r#"SELECT id AS "id!: i64", start_ms AS "start_ms!: i64", end_ms AS "end_ms!: i64",
                      speaker_id AS "speaker_id?: i64", text AS "text!: String"
               FROM meeting_segments WHERE meeting_id = ?1 ORDER BY start_ms, id"#,
            id
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        let summary = sqlx::query!(
            r#"SELECT body AS "body!: String", action_items AS "action_items!: String",
                      model AS "model!: String", created_at AS "created_at!: i64"
               FROM meeting_summaries WHERE meeting_id = ?1"#,
            id
        )
        .fetch_optional(&inner.pool)
        .await
        .map_err(AppError::database)?
        .map(|row| {
            Ok::<_, AppError>(MeetingSummary {
                text: row.body,
                action_items: serde_json::from_str::<Vec<ActionItem>>(&row.action_items)
                    .map_err(AppError::database)?,
                model: row.model,
                created_at: row.created_at,
            })
        })
        .transpose()?;
        let chat_rows = sqlx::query!(
            r#"SELECT id AS "id!: i64", role AS "role!: String", body AS "body!: String",
                      citations AS "citations!: String", created_at AS "created_at!: i64"
               FROM meeting_chat WHERE meeting_id = ?1 ORDER BY id"#,
            id
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        let mut chat = Vec::with_capacity(chat_rows.len());
        for row in chat_rows {
            chat.push(ChatMessage {
                id: row.id,
                role: parse_role(&row.role)?,
                text: row.body,
                citations: serde_json::from_str::<Vec<Citation>>(&row.citations)
                    .map_err(AppError::database)?,
                created_at: row.created_at,
            });
        }
        let translation_rows = sqlx::query!(
            r#"SELECT id AS "id!: i64", language AS "language!: String", model AS "model!: String",
                      created_at AS "created_at!: i64"
               FROM meeting_translations WHERE meeting_id = ?1 ORDER BY language"#,
            id
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        let translated = sqlx::query!(
            r#"SELECT ts.translation_id AS "translation_id!: i64",
                      ts.segment_id AS "segment_id!: i64", ts.body AS "body!: String"
               FROM meeting_translation_segments ts
               JOIN meeting_translations t ON t.id = ts.translation_id
               JOIN meeting_segments s ON s.id = ts.segment_id
               WHERE t.meeting_id = ?1
               ORDER BY ts.translation_id, s.start_ms, s.id"#,
            id
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        let translations = translation_rows
            .into_iter()
            .map(|row| Translation {
                segments: translated
                    .iter()
                    .filter(|segment| segment.translation_id == row.id)
                    .map(|segment| TranslatedSegment {
                        segment_id: segment.segment_id,
                        text: segment.body.clone(),
                    })
                    .collect(),
                language: row.language,
                model: row.model,
                created_at: row.created_at,
            })
            .collect();
        Ok(MeetingDetail {
            meeting,
            speakers,
            segments,
            summary,
            chat,
            translations,
        })
    }

    /// Records when the recording stopped and how long it is.
    pub async fn finish_meeting_recording(
        &self,
        id: i64,
        ended_at: i64,
        duration_ms: i64,
    ) -> AppResult<Meeting> {
        let duration_ms = duration_ms.max(0);
        let inner = self.read().await;
        let updated = sqlx::query!(
            "UPDATE meetings SET ended_at = ?1, duration_ms = ?2 WHERE id = ?3",
            ended_at,
            duration_ms,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(not_found(id));
        }
        stored_meeting(&inner.pool, id).await
    }

    /// Moves a meeting to `status`; `error` replaces the stored error (`None` clears it).
    pub async fn set_meeting_status(
        &self,
        id: i64,
        status: MeetingStatus,
        error: Option<&str>,
    ) -> AppResult<Meeting> {
        let status = status.as_str();
        let inner = self.read().await;
        let updated = sqlx::query!(
            "UPDATE meetings SET status = ?1, error = ?2 WHERE id = ?3",
            status,
            error,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(not_found(id));
        }
        stored_meeting(&inner.pool, id).await
    }

    /// Records a problem without changing the meeting's status (system audio that stopped
    /// mid-recording while the microphone carries on).
    pub async fn set_meeting_error(&self, id: i64, error: &str) -> AppResult<Meeting> {
        let inner = self.read().await;
        let updated = sqlx::query!("UPDATE meetings SET error = ?1 WHERE id = ?2", error, id)
            .execute(&inner.pool)
            .await
            .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(not_found(id));
        }
        stored_meeting(&inner.pool, id).await
    }

    /// Deletes the meeting and everything it owns; returns its audio folder for the caller to
    /// remove.
    pub async fn delete_meeting(&self, id: i64) -> AppResult<String> {
        let inner = self.read().await;
        let audio_dir = sqlx::query_scalar!(
            r#"DELETE FROM meetings WHERE id = ?1 RETURNING audio_dir AS "audio_dir!: String""#,
            id
        )
        .fetch_optional(&inner.pool)
        .await
        .map_err(AppError::database)?;
        audio_dir.ok_or_else(|| not_found(id))
    }

    /// Startup recovery: meetings a quit or crash left `recording` or `processing` become
    /// `failed` with an explanation, keeping their audio for a retry. Returns them; those with
    /// no `ended_at` were still recording, so their length is unknown until the caller measures
    /// the kept audio.
    pub async fn recover_interrupted_meetings(&self) -> AppResult<Vec<Meeting>> {
        let recording = MeetingStatus::Recording.as_str();
        let processing = MeetingStatus::Processing.as_str();
        let failed = MeetingStatus::Failed.as_str();
        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let ids = sqlx::query_scalar!(
            r#"UPDATE meetings SET status = ?1, error = ?2 WHERE status IN (?3, ?4)
               RETURNING id AS "id!: i64""#,
            failed,
            INTERRUPTED,
            recording,
            processing
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::database)?;
        let mut recovered = Vec::with_capacity(ids.len());
        for id in ids {
            recovered.push(stored_meeting(&mut *tx, id).await?);
        }
        tx.commit().await.map_err(AppError::database)?;
        recovered.sort_by_key(|meeting| meeting.id);
        Ok(recovered)
    }
}

async fn stored_meeting<'e, E>(executor: E, id: i64) -> AppResult<Meeting>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query_as!(
        MeetingRow,
        r#"SELECT m.id AS "id!: i64", m.title AS "title!: String",
                  m.started_at AS "started_at!: i64", m.ended_at AS "ended_at?: i64",
                  m.duration_ms AS "duration_ms!: i64", m.status AS "status!: String",
                  m.microphone AS "microphone!: bool", m.system_audio AS "system_audio!: bool",
                  m.language AS "language?: String", m.error AS "error?: String",
                  (SELECT COUNT(*) FROM meeting_segments s WHERE s.meeting_id = m.id)
                      AS "segment_count!: i64",
                  (SELECT COUNT(*) FROM meeting_speakers p WHERE p.meeting_id = m.id)
                      AS "speaker_count!: i64"
           FROM meetings m WHERE m.id = ?1"#,
        id
    )
    .fetch_optional(executor)
    .await
    .map_err(AppError::database)?
    .ok_or_else(|| not_found(id))?
    .into_meeting()
}

fn clean_title(title: &str) -> AppResult<String> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::invalid("Enter a meeting title"));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::invalid(format!(
            "Meeting titles can be at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(title.to_owned())
}

fn not_found(id: i64) -> AppError {
    AppError::NotFound(format!("Meeting {id}"))
}

fn parse_role(role: &str) -> AppResult<ChatRole> {
    ChatRole::parse(role)
        .ok_or_else(|| AppError::Database(format!("Meeting chat has unknown role “{role}”")))
}

/// A foreign-key failure on insert means the meeting was deleted meanwhile.
fn missing_meeting_or_database(error: sqlx::Error, meeting_id: i64) -> AppError {
    match &error {
        sqlx::Error::Database(db_error) if db_error.is_foreign_key_violation() => {
            not_found(meeting_id)
        }
        _ => AppError::database(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn open() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        (dir, db)
    }

    fn segment(start_ms: i64, speaker: Option<usize>, text: &str) -> NewSegment {
        NewSegment {
            start_ms,
            end_ms: start_ms + 1_000,
            speaker,
            text: text.to_owned(),
        }
    }

    async fn ready_meeting(db: &Db) -> MeetingDetail {
        let meeting = db
            .create_meeting("Weekly sync", 1_000, true, false, "/tmp/m1")
            .await
            .unwrap();
        db.replace_meeting_transcript(
            meeting.id,
            "parakeet-tdt-v3",
            Some("en"),
            &[segment(0, Some(4), "Hello"), segment(2_000, Some(9), "Hi")],
        )
        .await
        .unwrap();
        db.meeting_detail(meeting.id).await.unwrap()
    }

    #[tokio::test]
    async fn lists_newest_first_with_counts_and_status_changes() {
        let (_dir, db) = open().await;
        let older = db
            .create_meeting(" Standup ", 1_000, true, false, "/tmp/a")
            .await
            .unwrap();
        let newer = db
            .create_meeting("Review", 5_000, true, true, "/tmp/b")
            .await
            .unwrap();
        assert_eq!(older.title, "Standup");
        assert_eq!(older.status, MeetingStatus::Recording);
        assert_eq!(older.ended_at, None);
        assert!(newer.system_audio);
        assert!(matches!(
            db.create_meeting("  ", 1, true, false, "/tmp/c").await,
            Err(AppError::InvalidInput(_))
        ));

        let finished = db
            .finish_meeting_recording(older.id, 61_000, 60_000)
            .await
            .unwrap();
        assert_eq!(
            (finished.ended_at, finished.duration_ms),
            (Some(61_000), 60_000)
        );
        let processing = db
            .set_meeting_status(older.id, MeetingStatus::Processing, None)
            .await
            .unwrap();
        assert_eq!(processing.status, MeetingStatus::Processing);
        let failed = db
            .set_meeting_status(older.id, MeetingStatus::Failed, Some("Diarizer missing"))
            .await
            .unwrap();
        assert_eq!(failed.error.as_deref(), Some("Diarizer missing"));
        db.replace_meeting_transcript(older.id, "m", None, &[segment(0, Some(0), "a")])
            .await
            .unwrap();

        let listed = db.meetings().await.unwrap();
        let ids: Vec<i64> = listed.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![newer.id, older.id]);
        assert_eq!((listed[1].segment_count, listed[1].speaker_count), (1, 1));
        assert_eq!(listed[1].status, MeetingStatus::Ready);
        assert_eq!(listed[1].error, None);
        assert_eq!(db.meeting_audio_dir(newer.id).await.unwrap(), "/tmp/b");
        assert!(matches!(db.meeting(999).await, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn transcript_labels_speakers_by_first_appearance() {
        let (_dir, db) = open().await;
        let meeting = db
            .create_meeting("Sync", 1, true, false, "/tmp/x")
            .await
            .unwrap();
        // Given out of order and with arbitrary diarizer indices.
        let stored = db
            .replace_meeting_transcript(
                meeting.id,
                "whisper-large-v3-turbo",
                Some("en"),
                &[
                    segment(4_000, Some(2), "third"),
                    segment(0, Some(7), "first"),
                    segment(2_000, Some(2), "second"),
                    segment(6_000, None, "unknown"),
                    segment(8_000, Some(7), "  "),
                ],
            )
            .await
            .unwrap();
        assert_eq!(stored.status, MeetingStatus::Ready);
        assert_eq!(stored.language.as_deref(), Some("en"));
        assert_eq!((stored.segment_count, stored.speaker_count), (4, 2));

        let detail = db.meeting_detail(meeting.id).await.unwrap();
        let labels: Vec<&str> = detail.speakers.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, vec!["Speaker 1", "Speaker 2"]);
        let speaker_of = |text: &str| {
            let segment = detail.segments.iter().find(|s| s.text == text).unwrap();
            segment.speaker_id.map(|id| {
                detail
                    .speakers
                    .iter()
                    .find(|s| s.id == id)
                    .unwrap()
                    .label
                    .clone()
            })
        };
        assert_eq!(speaker_of("first").as_deref(), Some("Speaker 1"));
        assert_eq!(speaker_of("second").as_deref(), Some("Speaker 2"));
        assert_eq!(speaker_of("third").as_deref(), Some("Speaker 2"));
        assert_eq!(speaker_of("unknown"), None);
        let texts: Vec<&str> = detail.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["first", "second", "third", "unknown"]);
    }

    #[tokio::test]
    async fn reprocessing_replaces_transcript_and_what_was_written_about_it() {
        let (_dir, db) = open().await;
        let detail = ready_meeting(&db).await;
        let id = detail.meeting.id;
        db.save_meeting_summary(id, "Old", &[], "qwen3-4b-instruct")
            .await
            .unwrap();
        db.add_meeting_chat(id, ChatRole::User, "Q", &[])
            .await
            .unwrap();
        db.replace_meeting_transcript(id, "m", None, &[segment(0, Some(1), "New")])
            .await
            .unwrap();
        let after = db.meeting_detail(id).await.unwrap();
        assert_eq!(after.segments.len(), 1);
        assert_eq!(after.speakers.len(), 1);
        assert_eq!(after.summary, None);
        assert!(after.chat.is_empty());
    }

    #[tokio::test]
    async fn renames_meeting_speaker_and_segment() {
        let (_dir, db) = open().await;
        let detail = ready_meeting(&db).await;
        let id = detail.meeting.id;
        assert_eq!(db.rename_meeting(id, "Retro").await.unwrap().title, "Retro");
        assert!(db.rename_meeting(id, "").await.is_err());

        let speaker = detail.speakers[0].id;
        let named = db.rename_speaker(id, speaker, " Priya ").await.unwrap();
        assert_eq!(named.name.as_deref(), Some("Priya"));
        assert_eq!(named.label, "Speaker 1");
        let cleared = db.rename_speaker(id, speaker, "   ").await.unwrap();
        assert_eq!(cleared.name, None);
        assert!(matches!(
            db.rename_speaker(id + 1, speaker, "X").await,
            Err(AppError::NotFound(_))
        ));

        let segment_id = detail.segments[0].id;
        let edited = db
            .update_meeting_segment(segment_id, " Hello there ")
            .await
            .unwrap();
        assert_eq!(edited.text, "Hello there");
        assert_eq!(edited.start_ms, 0);
        assert_eq!(db.segment_meeting(segment_id).await.unwrap(), id);
        assert!(db.update_meeting_segment(segment_id, " ").await.is_err());
        assert!(matches!(
            db.update_meeting_segment(9_999, "x").await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn summary_and_chat_round_trip_their_json() {
        let (_dir, db) = open().await;
        let detail = ready_meeting(&db).await;
        let id = detail.meeting.id;
        let items = vec![ActionItem {
            text: "Send notes".to_owned(),
            owner: Some("Priya".to_owned()),
            start_ms: Some(2_000),
        }];
        db.save_meeting_summary(id, "First", &[], "apple-intelligence")
            .await
            .unwrap();
        let saved = db
            .save_meeting_summary(id, "Second", &items, "qwen3-4b-instruct")
            .await
            .unwrap();

        let citation = Citation {
            segment_id: detail.segments[1].id,
            start_ms: 2_000,
        };
        db.add_meeting_chat(id, ChatRole::User, "Who sends notes?", &[])
            .await
            .unwrap();
        let answer = db
            .add_meeting_chat(id, ChatRole::Assistant, "Priya [#2].", &[citation])
            .await
            .unwrap();
        assert!(matches!(
            db.add_meeting_chat(id + 50, ChatRole::User, "?", &[]).await,
            Err(AppError::NotFound(_))
        ));

        let after = db.meeting_detail(id).await.unwrap();
        assert_eq!(after.summary, Some(saved));
        assert_eq!(
            after.summary.as_ref().map(|s| s.text.as_str()),
            Some("Second")
        );
        assert_eq!(after.chat.len(), 2);
        assert_eq!(after.chat[0].role, ChatRole::User);
        assert_eq!(after.chat[1], answer);
        assert_eq!(after.chat[1].citations, vec![citation]);
    }

    #[tokio::test]
    async fn a_new_translation_replaces_the_same_language_only() {
        let (_dir, db) = open().await;
        let detail = ready_meeting(&db).await;
        let id = detail.meeting.id;
        let lines = |a: &str, b: &str| {
            vec![
                TranslatedSegment {
                    segment_id: detail.segments[0].id,
                    text: a.to_owned(),
                },
                TranslatedSegment {
                    segment_id: detail.segments[1].id,
                    text: b.to_owned(),
                },
            ]
        };
        db.save_meeting_translation(id, "fr", "m", &lines("Bonjour", "Salut"))
            .await
            .unwrap();
        db.save_meeting_translation(id, "de", "m", &lines("Hallo", "Hi"))
            .await
            .unwrap();
        db.save_meeting_translation(id, "fr", "m2", &lines("Bonjour !", "Coucou"))
            .await
            .unwrap();

        let after = db.meeting_detail(id).await.unwrap();
        let summary: Vec<(&str, &str, Vec<&str>)> = after
            .translations
            .iter()
            .map(|t| {
                (
                    t.language.as_str(),
                    t.model.as_str(),
                    t.segments.iter().map(|s| s.text.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("de", "m", vec!["Hallo", "Hi"]),
                ("fr", "m2", vec!["Bonjour !", "Coucou"]),
            ]
        );

        // A segment of another meeting is refused and nothing is stored.
        let other = ready_meeting(&db).await;
        let foreign = vec![TranslatedSegment {
            segment_id: other.segments[0].id,
            text: "x".to_owned(),
        }];
        assert!(db
            .save_meeting_translation(id, "es", "m", &foreign)
            .await
            .is_err());
        let after = db.meeting_detail(id).await.unwrap();
        assert!(after.translations.iter().all(|t| t.language != "es"));
    }

    #[tokio::test]
    async fn deleting_cascades_and_returns_the_audio_folder() {
        let (_dir, db) = open().await;
        let detail = ready_meeting(&db).await;
        let id = detail.meeting.id;
        db.save_meeting_summary(id, "S", &[], "m").await.unwrap();
        db.add_meeting_chat(id, ChatRole::User, "Q", &[])
            .await
            .unwrap();
        let translated = vec![TranslatedSegment {
            segment_id: detail.segments[0].id,
            text: "Bonjour".to_owned(),
        }];
        db.save_meeting_translation(id, "fr", "m", &translated)
            .await
            .unwrap();

        assert_eq!(db.delete_meeting(id).await.unwrap(), "/tmp/m1");
        assert!(matches!(
            db.meeting_detail(id).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            db.delete_meeting(id).await,
            Err(AppError::NotFound(_))
        ));

        let inner = db.read().await;
        let left = sqlx::query_scalar!(
            r#"SELECT (SELECT COUNT(*) FROM meeting_speakers)
                    + (SELECT COUNT(*) FROM meeting_segments)
                    + (SELECT COUNT(*) FROM meeting_summaries)
                    + (SELECT COUNT(*) FROM meeting_chat)
                    + (SELECT COUNT(*) FROM meeting_translations)
                    + (SELECT COUNT(*) FROM meeting_translation_segments) AS "left!: i64""#
        )
        .fetch_one(&inner.pool)
        .await
        .unwrap();
        assert_eq!(left, 0);
    }

    #[tokio::test]
    async fn recovery_fails_interrupted_meetings_and_keeps_the_rest() {
        let (_dir, db) = open().await;
        let recording = db
            .create_meeting("A", 1, true, false, "/tmp/a")
            .await
            .unwrap();
        let processing = db
            .create_meeting("B", 2, true, false, "/tmp/b")
            .await
            .unwrap();
        db.finish_meeting_recording(processing.id, 10_000, 9_998)
            .await
            .unwrap();
        db.set_meeting_status(processing.id, MeetingStatus::Processing, None)
            .await
            .unwrap();
        let ready = ready_meeting(&db).await.meeting;

        let recovered = db.recover_interrupted_meetings().await.unwrap();
        let ids: Vec<i64> = recovered.iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![recording.id, processing.id]);
        for meeting in &recovered {
            assert_eq!(meeting.status, MeetingStatus::Failed);
            assert_eq!(meeting.error.as_deref(), Some(INTERRUPTED));
        }
        // Still recording when interrupted: no end yet, for the caller to measure.
        assert_eq!(recovered[0].ended_at, None);
        assert_eq!(recovered[1].duration_ms, 9_998);
        assert_eq!(
            db.meeting(ready.id).await.unwrap().status,
            MeetingStatus::Ready
        );
        assert_eq!(db.meeting_audio_dir(recording.id).await.unwrap(), "/tmp/a");
        assert!(db.recover_interrupted_meetings().await.unwrap().is_empty());
    }
}
