// FilePath: crates/sv-storage/src/meetings/transcript.rs
//! A meeting's processed transcript and what was written about it: storing a fresh transcript,
//! the user's corrections (title, speaker names, segment text), and the summary, chat and
//! translations the on-device models produced.

use super::*;

impl Db {
    /// Stores a freshly processed transcript in one transaction and marks the meeting ready.
    /// Speakers are created as "Speaker 1", "Speaker 2"… in order of first appearance. The
    /// previous transcript goes, and with it the summary, chat and translations that were
    /// written about it (their citations point at segments that no longer exist).
    pub async fn replace_meeting_transcript(
        &self,
        id: i64,
        model: &str,
        language: Option<&str>,
        segments: &[NewSegment],
    ) -> AppResult<Meeting> {
        let mut ordered: Vec<&NewSegment> = segments
            .iter()
            .filter(|segment| !segment.text.trim().is_empty())
            .collect();
        ordered.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
        let status = MeetingStatus::Ready.as_str();

        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let updated = sqlx::query!(
            "UPDATE meetings SET status = ?1, error = NULL, model = ?2, language = ?3
             WHERE id = ?4",
            status,
            model,
            language,
            id
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(not_found(id));
        }
        sqlx::query!("DELETE FROM meeting_summaries WHERE meeting_id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM meeting_chat WHERE meeting_id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM meeting_translations WHERE meeting_id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM meeting_segments WHERE meeting_id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM meeting_speakers WHERE meeting_id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;

        // Diarizer index → stored speaker id, created on first appearance.
        let mut speaker_ids: Vec<(usize, i64)> = Vec::new();
        for segment in ordered {
            let speaker_id = match segment.speaker {
                None => None,
                Some(index) => match speaker_ids.iter().find(|(known, _)| *known == index) {
                    Some(&(_, speaker_id)) => Some(speaker_id),
                    None => {
                        let label = format!("Speaker {}", speaker_ids.len() + 1);
                        let speaker_id = sqlx::query!(
                            "INSERT INTO meeting_speakers (meeting_id, label) VALUES (?1, ?2)",
                            id,
                            label
                        )
                        .execute(&mut *tx)
                        .await
                        .map_err(AppError::database)?
                        .last_insert_rowid();
                        speaker_ids.push((index, speaker_id));
                        Some(speaker_id)
                    }
                },
            };
            let start_ms = segment.start_ms.max(0);
            let end_ms = segment.end_ms.max(start_ms);
            let text = segment.text.trim();
            sqlx::query!(
                "INSERT INTO meeting_segments (meeting_id, start_ms, end_ms, speaker_id, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                id,
                start_ms,
                end_ms,
                speaker_id,
                text
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        }
        let meeting = stored_meeting(&mut *tx, id).await?;
        tx.commit().await.map_err(AppError::database)?;
        Ok(meeting)
    }

    pub async fn rename_meeting(&self, id: i64, title: &str) -> AppResult<Meeting> {
        let title = clean_title(title)?;
        let inner = self.read().await;
        let updated = sqlx::query!("UPDATE meetings SET title = ?1 WHERE id = ?2", title, id)
            .execute(&inner.pool)
            .await
            .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(not_found(id));
        }
        stored_meeting(&inner.pool, id).await
    }

    /// Names a speaker of a meeting; a blank name goes back to the "Speaker N" label.
    pub async fn rename_speaker(
        &self,
        meeting_id: i64,
        speaker_id: i64,
        name: &str,
    ) -> AppResult<Speaker> {
        let name = name.trim();
        if name.chars().count() > MAX_SPEAKER_NAME_CHARS {
            return Err(AppError::invalid(format!(
                "Speaker names can be at most {MAX_SPEAKER_NAME_CHARS} characters"
            )));
        }
        let name = (!name.is_empty()).then_some(name);
        let inner = self.read().await;
        let updated = sqlx::query!(
            "UPDATE meeting_speakers SET name = ?1 WHERE id = ?2 AND meeting_id = ?3",
            name,
            speaker_id,
            meeting_id
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound("Speaker".to_owned()));
        }
        sqlx::query_as!(
            Speaker,
            r#"SELECT id AS "id!: i64", label AS "label!: String", name AS "name?: String"
               FROM meeting_speakers WHERE id = ?1"#,
            speaker_id
        )
        .fetch_one(&inner.pool)
        .await
        .map_err(AppError::database)
    }

    /// Corrects the text of one transcript segment.
    pub async fn update_meeting_segment(&self, id: i64, text: &str) -> AppResult<Segment> {
        let text = text.trim();
        if text.is_empty() {
            return Err(AppError::invalid("A transcript segment cannot be empty"));
        }
        if text.chars().count() > MAX_SEGMENT_CHARS {
            return Err(AppError::invalid(format!(
                "A transcript segment can be at most {MAX_SEGMENT_CHARS} characters"
            )));
        }
        let inner = self.read().await;
        let updated = sqlx::query!(
            "UPDATE meeting_segments SET text = ?1 WHERE id = ?2",
            text,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound("Transcript segment".to_owned()));
        }
        sqlx::query_as!(
            Segment,
            r#"SELECT id AS "id!: i64", start_ms AS "start_ms!: i64", end_ms AS "end_ms!: i64",
                      speaker_id AS "speaker_id?: i64", text AS "text!: String"
               FROM meeting_segments WHERE id = ?1"#,
            id
        )
        .fetch_one(&inner.pool)
        .await
        .map_err(AppError::database)
    }

    /// The meeting a transcript segment belongs to.
    pub async fn segment_meeting(&self, segment_id: i64) -> AppResult<i64> {
        let inner = self.read().await;
        sqlx::query_scalar!(
            r#"SELECT meeting_id AS "meeting_id!: i64" FROM meeting_segments WHERE id = ?1"#,
            segment_id
        )
        .fetch_optional(&inner.pool)
        .await
        .map_err(AppError::database)?
        .ok_or_else(|| AppError::NotFound("Transcript segment".to_owned()))
    }

    /// Stores (or replaces) the meeting's summary.
    pub async fn save_meeting_summary(
        &self,
        meeting_id: i64,
        text: &str,
        action_items: &[ActionItem],
        model: &str,
    ) -> AppResult<MeetingSummary> {
        let items = serde_json::to_string(action_items).map_err(AppError::database)?;
        let now = chrono::Utc::now().timestamp_millis();
        let inner = self.read().await;
        sqlx::query!(
            "INSERT INTO meeting_summaries (meeting_id, body, action_items, model, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (meeting_id) DO UPDATE SET body = excluded.body,
                 action_items = excluded.action_items, model = excluded.model,
                 created_at = excluded.created_at",
            meeting_id,
            text,
            items,
            model,
            now
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| missing_meeting_or_database(error, meeting_id))?;
        Ok(MeetingSummary {
            text: text.to_owned(),
            action_items: action_items.to_vec(),
            model: model.to_owned(),
            created_at: now,
        })
    }

    pub async fn add_meeting_chat(
        &self,
        meeting_id: i64,
        role: ChatRole,
        text: &str,
        citations: &[Citation],
    ) -> AppResult<ChatMessage> {
        let encoded = serde_json::to_string(citations).map_err(AppError::database)?;
        let role_text = role.as_str();
        let now = chrono::Utc::now().timestamp_millis();
        let inner = self.read().await;
        let id = sqlx::query!(
            "INSERT INTO meeting_chat (meeting_id, role, body, citations, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            meeting_id,
            role_text,
            text,
            encoded,
            now
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| missing_meeting_or_database(error, meeting_id))?
        .last_insert_rowid();
        Ok(ChatMessage {
            id,
            role,
            text: text.to_owned(),
            citations: citations.to_vec(),
            created_at: now,
        })
    }

    /// Stores a translation, replacing an earlier one into the same language. Every segment
    /// must belong to the meeting.
    pub async fn save_meeting_translation(
        &self,
        meeting_id: i64,
        language: &str,
        model: &str,
        segments: &[TranslatedSegment],
    ) -> AppResult<Translation> {
        let now = chrono::Utc::now().timestamp_millis();
        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        sqlx::query!(
            "DELETE FROM meeting_translations WHERE meeting_id = ?1 AND language = ?2",
            meeting_id,
            language
        )
        .execute(&mut *tx)
        .await
        .map_err(AppError::database)?;
        let translation_id = sqlx::query!(
            "INSERT INTO meeting_translations (meeting_id, language, model, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            meeting_id,
            language,
            model,
            now
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| missing_meeting_or_database(error, meeting_id))?
        .last_insert_rowid();
        for segment in segments {
            let inserted = sqlx::query!(
                "INSERT INTO meeting_translation_segments (translation_id, segment_id, body)
                 SELECT ?1, id, ?2 FROM meeting_segments WHERE id = ?3 AND meeting_id = ?4",
                translation_id,
                segment.text,
                segment.segment_id,
                meeting_id
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
            if inserted.rows_affected() == 0 {
                return Err(AppError::NotFound("Transcript segment".to_owned()));
            }
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(Translation {
            language: language.to_owned(),
            model: model.to_owned(),
            created_at: now,
            segments: segments.to_vec(),
        })
    }
}
