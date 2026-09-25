// FilePath: crates/sv-storage/src/history.rs
//! Dictation history rows. Storage derives the word statistics and the app category so every
//! writer computes them the same way.

use sqlx::SqlitePool;
use sv_domain::{
    categorize, text_stats, AppCategory, AppError, AppResult, HistoryEntry, HistoryStatus,
    NewHistory, Style,
};

use crate::db::Db;

const MAX_PAGE_SIZE: i64 = 500;

#[derive(Debug)]
struct HistoryRow {
    id: i64,
    created_at: i64,
    status: String,
    raw_text: String,
    final_text: String,
    error: Option<String>,
    model: String,
    language: Option<String>,
    style: String,
    audio_ms: i64,
    latency_ms: i64,
    word_count: i64,
    dictionary_fixes: i64,
    words_corrected: i64,
    app_name: Option<String>,
    bundle_id: Option<String>,
    app_category: String,
    audio_path: Option<String>,
}

impl HistoryRow {
    fn into_entry(self) -> AppResult<HistoryEntry> {
        let status = HistoryStatus::parse(&self.status).ok_or_else(|| {
            AppError::Database(format!(
                "Dictation {} has unknown status “{}”",
                self.id, self.status
            ))
        })?;
        Ok(HistoryEntry {
            id: self.id,
            created_at: self.created_at,
            status,
            raw_text: self.raw_text,
            text: self.final_text,
            error: self.error,
            model: self.model,
            language: self.language,
            style: parse_style(&self.style),
            audio_ms: self.audio_ms,
            latency_ms: self.latency_ms,
            word_count: self.word_count,
            dictionary_fixes: self.dictionary_fixes,
            words_corrected: self.words_corrected,
            app_name: self.app_name,
            bundle_id: self.bundle_id,
            app_category: AppCategory::parse(&self.app_category),
            can_retry: status == HistoryStatus::Failed && self.audio_path.is_some(),
        })
    }
}

/// Columns computed from a `NewHistory` rather than supplied by the caller.
#[derive(Debug)]
struct Derived {
    status: &'static str,
    style: &'static str,
    word_count: i64,
    words_corrected: i64,
    app_category: &'static str,
}

impl Derived {
    fn of(entry: &NewHistory) -> Self {
        let words_corrected = text_stats::words_corrected(&entry.raw_text, &entry.final_text);
        Self {
            status: entry.status.as_str(),
            style: style_str(entry.style),
            word_count: saturating_i64(text_stats::word_count(&entry.final_text)),
            words_corrected: saturating_i64(words_corrected),
            app_category: categorize(entry.bundle_id.as_deref(), entry.app_name.as_deref())
                .as_str(),
        }
    }
}

impl Db {
    pub async fn insert_history(&self, entry: NewHistory) -> AppResult<HistoryEntry> {
        let inner = self.read().await;
        let derived = Derived::of(&entry);
        let id = sqlx::query!(
            "INSERT INTO history (
                created_at, status, raw_text, final_text, error, model, language, style,
                audio_ms, latency_ms, word_count, dictionary_fixes, words_corrected,
                app_name, bundle_id, app_category, audio_path
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            entry.created_at,
            derived.status,
            entry.raw_text,
            entry.final_text,
            entry.error,
            entry.model,
            entry.language,
            derived.style,
            entry.audio_ms,
            entry.latency_ms,
            derived.word_count,
            entry.dictionary_fixes,
            derived.words_corrected,
            entry.app_name,
            entry.bundle_id,
            derived.app_category,
            entry.audio_path
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?
        .last_insert_rowid();
        stored_entry(&inner.pool, id).await
    }

    /// Replaces every field of an existing row, e.g. after a successful retry.
    pub async fn update_history(&self, id: i64, entry: NewHistory) -> AppResult<HistoryEntry> {
        let inner = self.read().await;
        let derived = Derived::of(&entry);
        let updated = sqlx::query!(
            "UPDATE history SET
                created_at = ?1, status = ?2, raw_text = ?3, final_text = ?4, error = ?5,
                model = ?6, language = ?7, style = ?8, audio_ms = ?9, latency_ms = ?10,
                word_count = ?11, dictionary_fixes = ?12, words_corrected = ?13, app_name = ?14,
                bundle_id = ?15, app_category = ?16, audio_path = ?17
             WHERE id = ?18",
            entry.created_at,
            derived.status,
            entry.raw_text,
            entry.final_text,
            entry.error,
            entry.model,
            entry.language,
            derived.style,
            entry.audio_ms,
            entry.latency_ms,
            derived.word_count,
            entry.dictionary_fixes,
            derived.words_corrected,
            entry.app_name,
            entry.bundle_id,
            derived.app_category,
            entry.audio_path,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(AppError::database)?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("Dictation {id}")));
        }
        stored_entry(&inner.pool, id).await
    }

    pub async fn history_entry(&self, id: i64) -> AppResult<Option<HistoryEntry>> {
        let inner = self.read().await;
        fetch_entry(&inner.pool, id).await
    }

    pub async fn history_audio_path(&self, id: i64) -> AppResult<Option<String>> {
        let inner = self.read().await;
        let path = sqlx::query_scalar!(
            r#"SELECT audio_path AS "audio_path?: String" FROM history WHERE id = ?1"#,
            id
        )
        .fetch_optional(&inner.pool)
        .await
        .map_err(AppError::database)?;
        Ok(path.flatten())
    }

    /// Newest first. `query` is a case-insensitive substring match over the pasted text, the raw
    /// transcript and the app name; `before_id` continues from the last row of the previous page.
    pub async fn list_history(
        &self,
        query: Option<String>,
        limit: i64,
        before_id: Option<i64>,
    ) -> AppResult<Vec<HistoryEntry>> {
        let inner = self.read().await;
        let pattern = query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(|query| format!("%{}%", escape_like(query)));
        let limit = limit.clamp(1, MAX_PAGE_SIZE);
        let rows = sqlx::query_as!(
            HistoryRow,
            r#"SELECT
                id AS "id!: i64", created_at AS "created_at!: i64", status AS "status!: String",
                raw_text AS "raw_text!: String", final_text AS "final_text!: String",
                error AS "error?: String", model AS "model!: String",
                language AS "language?: String", style AS "style!: String",
                audio_ms AS "audio_ms!: i64", latency_ms AS "latency_ms!: i64",
                word_count AS "word_count!: i64", dictionary_fixes AS "dictionary_fixes!: i64",
                words_corrected AS "words_corrected!: i64", app_name AS "app_name?: String",
                bundle_id AS "bundle_id?: String", app_category AS "app_category!: String",
                audio_path AS "audio_path?: String"
               FROM history
               WHERE (?1 IS NULL
                      OR final_text LIKE ?1 ESCAPE '\'
                      OR raw_text LIKE ?1 ESCAPE '\'
                      OR app_name LIKE ?1 ESCAPE '\')
                 AND (?2 IS NULL OR id < ?2)
               ORDER BY id DESC
               LIMIT ?3"#,
            pattern,
            before_id,
            limit
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        rows.into_iter().map(HistoryRow::into_entry).collect()
    }

    /// Returns the retained audio path, if any, so the caller can delete the file.
    pub async fn delete_history(&self, id: i64) -> AppResult<Option<String>> {
        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let path = sqlx::query_scalar!(
            r#"SELECT audio_path AS "audio_path?: String" FROM history WHERE id = ?1"#,
            id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM history WHERE id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        tx.commit().await.map_err(AppError::database)?;
        Ok(path.flatten())
    }

    /// Returns every retained audio path so the caller can delete the files.
    pub async fn clear_history(&self) -> AppResult<Vec<String>> {
        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let paths = sqlx::query_scalar!(
            r#"SELECT audio_path AS "audio_path!: String" FROM history
               WHERE audio_path IS NOT NULL"#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM history")
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        tx.commit().await.map_err(AppError::database)?;
        Ok(paths)
    }
}

async fn fetch_entry(pool: &SqlitePool, id: i64) -> AppResult<Option<HistoryEntry>> {
    let row = sqlx::query_as!(
        HistoryRow,
        r#"SELECT
            id AS "id!: i64", created_at AS "created_at!: i64", status AS "status!: String",
            raw_text AS "raw_text!: String", final_text AS "final_text!: String",
            error AS "error?: String", model AS "model!: String",
            language AS "language?: String", style AS "style!: String",
            audio_ms AS "audio_ms!: i64", latency_ms AS "latency_ms!: i64",
            word_count AS "word_count!: i64", dictionary_fixes AS "dictionary_fixes!: i64",
            words_corrected AS "words_corrected!: i64", app_name AS "app_name?: String",
            bundle_id AS "bundle_id?: String", app_category AS "app_category!: String",
            audio_path AS "audio_path?: String"
           FROM history WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::database)?;
    row.map(HistoryRow::into_entry).transpose()
}

async fn stored_entry(pool: &SqlitePool, id: i64) -> AppResult<HistoryEntry> {
    fetch_entry(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Dictation {id}")))
}

fn style_str(style: Style) -> &'static str {
    match style {
        Style::Formal => "formal",
        Style::Casual => "casual",
    }
}

fn parse_style(value: &str) -> Style {
    match value {
        "casual" => Style::Casual,
        _ => Style::Formal,
    }
}

fn saturating_i64(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// Escapes LIKE wildcards so a search for `50%` or `snake_case` matches literally.
fn escape_like(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for ch in query.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(final_text: &str) -> NewHistory {
        NewHistory {
            created_at: 1_750_000_000_000,
            status: HistoryStatus::Pasted,
            raw_text: final_text.to_owned(),
            final_text: final_text.to_owned(),
            error: None,
            model: "groq-whisper".to_owned(),
            language: Some("en".to_owned()),
            style: Style::Formal,
            audio_ms: 4_000,
            latency_ms: 300,
            dictionary_fixes: 0,
            app_name: None,
            bundle_id: None,
            audio_path: None,
        }
    }

    async fn open() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn insert_derives_statistics_and_category() {
        let (_dir, db) = open().await;
        let stored = db
            .insert_history(NewHistory {
                raw_text: "um I gonna go".to_owned(),
                final_text: "I am going to go.".to_owned(),
                style: Style::Casual,
                app_name: Some("Slack".to_owned()),
                bundle_id: Some("com.tinyspeck.slackmacgap".to_owned()),
                ..entry("")
            })
            .await
            .unwrap();
        assert_eq!(stored.word_count, 5);
        assert_eq!(stored.words_corrected, 4);
        assert_eq!(stored.app_category, AppCategory::WorkMessages);
        assert_eq!(stored.style, Style::Casual);
        assert_eq!(stored.text, "I am going to go.");
        assert!(!stored.can_retry);
        assert_eq!(db.history_entry(stored.id).await.unwrap(), Some(stored));
    }

    #[tokio::test]
    async fn failed_entry_with_audio_can_be_retried_and_updated() {
        let (_dir, db) = open().await;
        let failed = db
            .insert_history(NewHistory {
                status: HistoryStatus::Failed,
                error: Some("Groq timed out".to_owned()),
                audio_path: Some("/tmp/a.wav".to_owned()),
                ..entry("")
            })
            .await
            .unwrap();
        assert!(failed.can_retry);
        assert_eq!(
            db.history_audio_path(failed.id).await.unwrap().as_deref(),
            Some("/tmp/a.wav")
        );

        let retried = db
            .update_history(failed.id, entry("Ship it today"))
            .await
            .unwrap();
        assert_eq!(retried.id, failed.id);
        assert_eq!(retried.status, HistoryStatus::Pasted);
        assert_eq!(retried.word_count, 3);
        assert!(!retried.can_retry);
        assert_eq!(db.history_audio_path(failed.id).await.unwrap(), None);

        let missing = db
            .update_history(failed.id + 100, entry("x"))
            .await
            .unwrap_err();
        assert!(matches!(missing, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn search_is_case_insensitive_and_literal() {
        let (_dir, db) = open().await;
        db.insert_history(entry("Deploy ArgoCD tonight"))
            .await
            .unwrap();
        db.insert_history(entry("Progress is 50% done"))
            .await
            .unwrap();
        db.insert_history(NewHistory {
            app_name: Some("Slack".to_owned()),
            ..entry("hello")
        })
        .await
        .unwrap();
        db.insert_history(NewHistory {
            raw_text: "snake_case name".to_owned(),
            ..entry("x")
        })
        .await
        .unwrap();

        let texts = |entries: Vec<HistoryEntry>| -> Vec<String> {
            entries.into_iter().map(|entry| entry.text).collect()
        };
        let db = &db;
        let search = move |query: &str| db.list_history(Some(query.to_owned()), 50, None);
        assert_eq!(
            texts(search("argocd").await.unwrap()),
            vec!["Deploy ArgoCD tonight"]
        );
        assert_eq!(texts(search("SLACK").await.unwrap()), vec!["hello"]);
        assert_eq!(
            texts(search("50%").await.unwrap()),
            vec!["Progress is 50% done"]
        );
        assert_eq!(texts(search("e_c").await.unwrap()), vec!["x"]);
        assert!(search("0% d_").await.unwrap().is_empty());
        assert_eq!(
            db.list_history(Some("  ".to_owned()), 50, None)
                .await
                .unwrap()
                .len(),
            4
        );
    }

    #[tokio::test]
    async fn pages_newest_first_by_id() {
        let (_dir, db) = open().await;
        let mut ids = Vec::new();
        for n in 0..5 {
            ids.push(
                db.insert_history(entry(&format!("note {n}")))
                    .await
                    .unwrap()
                    .id,
            );
        }
        let first: Vec<i64> = db
            .list_history(None, 2, None)
            .await
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(first, vec![ids[4], ids[3]]);
        let second: Vec<i64> = db
            .list_history(None, 2, Some(ids[3]))
            .await
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(second, vec![ids[2], ids[1]]);
        let last: Vec<i64> = db
            .list_history(None, 2, Some(ids[1]))
            .await
            .unwrap()
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(last, vec![ids[0]]);
    }

    #[tokio::test]
    async fn delete_and_clear_return_audio_paths() {
        let (_dir, db) = open().await;
        let with_audio = NewHistory {
            status: HistoryStatus::Failed,
            audio_path: Some("/tmp/one.wav".to_owned()),
            ..entry("")
        };
        let first = db.insert_history(with_audio.clone()).await.unwrap();
        let plain = db.insert_history(entry("kept")).await.unwrap();
        assert_eq!(
            db.delete_history(first.id).await.unwrap().as_deref(),
            Some("/tmp/one.wav")
        );
        assert_eq!(db.delete_history(plain.id).await.unwrap(), None);
        assert_eq!(db.history_entry(first.id).await.unwrap(), None);

        db.insert_history(NewHistory {
            audio_path: Some("/tmp/two.wav".to_owned()),
            ..with_audio
        })
        .await
        .unwrap();
        db.insert_history(entry("no audio")).await.unwrap();
        assert_eq!(db.clear_history().await.unwrap(), vec!["/tmp/two.wav"]);
        assert!(db.list_history(None, 50, None).await.unwrap().is_empty());
    }
}
