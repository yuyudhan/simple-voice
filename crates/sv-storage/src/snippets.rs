// FilePath: crates/sv-storage/src/snippets.rs
//! Personal snippets: spoken trigger phrases that dictation expands into saved text. The trigger
//! is unique ignoring case (the column is `COLLATE NOCASE UNIQUE`); the text is stored exactly as
//! given so multiline payloads paste back unchanged.

use sqlx::{Executor, Sqlite};
use sv_domain::snippets::{
    SNIPPET_TEXT_MAX_CHARS, SNIPPET_TRIGGER_MAX_CHARS, SNIPPET_TRIGGER_MAX_WORDS,
};
use sv_domain::{AppError, AppResult, Snippet};

use crate::db::Db;

#[derive(Debug)]
struct SnippetRow {
    id: i64,
    trigger_phrase: String,
    body: String,
    created_at: i64,
    updated_at: i64,
}

impl SnippetRow {
    fn into_snippet(self) -> Snippet {
        Snippet {
            id: self.id,
            trigger: self.trigger_phrase,
            text: self.body,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

impl Db {
    /// Every snippet, ordered by trigger A–Z (case-insensitive).
    pub async fn snippets(&self) -> AppResult<Vec<Snippet>> {
        let inner = self.read().await;
        let rows = sqlx::query_as!(
            SnippetRow,
            r#"SELECT id AS "id!: i64", trigger_phrase AS "trigger_phrase!: String",
                      body AS "body!: String", created_at AS "created_at!: i64",
                      updated_at AS "updated_at!: i64"
               FROM snippets ORDER BY trigger_phrase COLLATE NOCASE, id"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        Ok(rows.into_iter().map(SnippetRow::into_snippet).collect())
    }

    /// Saves a new snippet. The trigger is trimmed; the text is kept verbatim.
    pub async fn add_snippet(&self, trigger: String, text: String) -> AppResult<Snippet> {
        let trigger = clean_trigger(&trigger)?;
        check_text(&text)?;
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let id = sqlx::query!(
            "INSERT INTO snippets (trigger_phrase, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            trigger,
            text,
            now,
            now
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| duplicate_or_database(error, &trigger))?
        .last_insert_rowid();
        stored_snippet(&inner.pool, id).await
    }

    pub async fn update_snippet(
        &self,
        id: i64,
        trigger: String,
        text: String,
    ) -> AppResult<Snippet> {
        let trigger = clean_trigger(&trigger)?;
        check_text(&text)?;
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let updated = sqlx::query!(
            "UPDATE snippets SET trigger_phrase = ?1, body = ?2, updated_at = ?3 WHERE id = ?4",
            trigger,
            text,
            now,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| duplicate_or_database(error, &trigger))?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound("Snippet".to_owned()));
        }
        stored_snippet(&inner.pool, id).await
    }

    /// Deleting a snippet that is already gone is not an error.
    pub async fn delete_snippet(&self, id: i64) -> AppResult<()> {
        let inner = self.read().await;
        sqlx::query!("DELETE FROM snippets WHERE id = ?1", id)
            .execute(&inner.pool)
            .await
            .map_err(AppError::database)?;
        Ok(())
    }
}

async fn stored_snippet<'e, E>(executor: E, id: i64) -> AppResult<Snippet>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query_as!(
        SnippetRow,
        r#"SELECT id AS "id!: i64", trigger_phrase AS "trigger_phrase!: String",
                  body AS "body!: String", created_at AS "created_at!: i64",
                  updated_at AS "updated_at!: i64"
           FROM snippets WHERE id = ?1"#,
        id
    )
    .fetch_optional(executor)
    .await
    .map_err(AppError::database)?
    .map(SnippetRow::into_snippet)
    .ok_or_else(|| AppError::NotFound("Snippet".to_owned()))
}

/// The trimmed trigger, or why it cannot be spoken as one.
fn clean_trigger(trigger: &str) -> AppResult<String> {
    let trigger = trigger.trim();
    if trigger.is_empty() {
        return Err(AppError::invalid("Enter a trigger phrase"));
    }
    if trigger.chars().count() > SNIPPET_TRIGGER_MAX_CHARS {
        return Err(AppError::invalid(format!(
            "A trigger can be at most {SNIPPET_TRIGGER_MAX_CHARS} characters"
        )));
    }
    if trigger.split_whitespace().count() > SNIPPET_TRIGGER_MAX_WORDS {
        return Err(AppError::invalid(format!(
            "A trigger can be at most {SNIPPET_TRIGGER_MAX_WORDS} words"
        )));
    }
    if !trigger.chars().any(char::is_alphanumeric) {
        return Err(AppError::invalid(
            "A trigger needs at least one letter or digit",
        ));
    }
    Ok(trigger.to_owned())
}

fn check_text(text: &str) -> AppResult<()> {
    if text.trim().is_empty() {
        return Err(AppError::invalid("Enter the text the trigger expands to"));
    }
    if text.chars().count() > SNIPPET_TEXT_MAX_CHARS {
        return Err(AppError::invalid(format!(
            "Snippet text can be at most {SNIPPET_TEXT_MAX_CHARS} characters"
        )));
    }
    Ok(())
}

fn duplicate_or_database(error: sqlx::Error, trigger: &str) -> AppError {
    match &error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            AppError::InvalidInput(format!(
                "Another snippet already uses the trigger “{trigger}”"
            ))
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

    fn invalid_message(error: AppError) -> String {
        match error {
            AppError::InvalidInput(message) => message,
            other => panic!("expected invalid input, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn snippets_are_listed_by_trigger_and_text_is_kept_verbatim() {
        let (_dir, db) = open().await;
        let address = "  221B Baker Street\n\tLondon NW1 6XE\n\n";
        let sig = db
            .add_snippet("  my sign off ".to_owned(), "Cheers,\nAnkur".to_owned())
            .await
            .unwrap();
        assert_eq!(sig.trigger, "my sign off");
        assert_eq!(sig.created_at, sig.updated_at);
        let home = db
            .add_snippet("Home Address".to_owned(), address.to_owned())
            .await
            .unwrap();
        assert_eq!(home.text, address);

        let listed = db.snippets().await.unwrap();
        assert_eq!(listed, vec![home, sig]);
    }

    #[tokio::test]
    async fn duplicate_triggers_are_rejected_ignoring_case() {
        let (_dir, db) = open().await;
        db.add_snippet("my email".to_owned(), "me@example.com".to_owned())
            .await
            .unwrap();
        let duplicate = db
            .add_snippet(" MY EMAIL ".to_owned(), "other@example.com".to_owned())
            .await
            .unwrap_err();
        assert!(invalid_message(duplicate).contains("trigger"));
        assert_eq!(db.snippets().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn invalid_triggers_and_texts_are_rejected() {
        let (_dir, db) = open().await;
        let text = || "body".to_owned();
        for trigger in [
            "   ".to_owned(),
            "?! ...".to_owned(),
            "one two three four five six seven".to_owned(),
            "x".repeat(SNIPPET_TRIGGER_MAX_CHARS + 1),
        ] {
            let error = db.add_snippet(trigger, text()).await.unwrap_err();
            assert!(matches!(error, AppError::InvalidInput(_)));
        }
        for body in [" \n\t ".to_owned(), "y".repeat(SNIPPET_TEXT_MAX_CHARS + 1)] {
            let error = db.add_snippet("ok".to_owned(), body).await.unwrap_err();
            assert!(matches!(error, AppError::InvalidInput(_)));
        }
        let six_words = db
            .add_snippet("one two three four five six".to_owned(), text())
            .await;
        assert!(six_words.is_ok());
        let longest = db
            .add_snippet(
                "z".repeat(SNIPPET_TRIGGER_MAX_CHARS),
                "y".repeat(SNIPPET_TEXT_MAX_CHARS),
            )
            .await;
        assert!(longest.is_ok());
    }

    #[tokio::test]
    async fn update_keeps_identity_and_rejects_collisions() {
        let (_dir, db) = open().await;
        let first = db
            .add_snippet("first".to_owned(), "one".to_owned())
            .await
            .unwrap();
        db.add_snippet("second".to_owned(), "two".to_owned())
            .await
            .unwrap();

        let updated = db
            .update_snippet(
                first.id,
                "First Line".to_owned(),
                "line 1\n  line 2".to_owned(),
            )
            .await
            .unwrap();
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.trigger, "First Line");
        assert_eq!(updated.text, "line 1\n  line 2");
        assert_eq!(updated.created_at, first.created_at);
        assert!(updated.updated_at >= first.updated_at);

        let recase = db
            .update_snippet(first.id, "FIRST LINE".to_owned(), "line 1".to_owned())
            .await
            .unwrap();
        assert_eq!(recase.trigger, "FIRST LINE");

        let clash = db
            .update_snippet(first.id, "Second".to_owned(), "x".to_owned())
            .await
            .unwrap_err();
        assert!(invalid_message(clash).contains("trigger"));

        let missing = db
            .update_snippet(9_999, "third".to_owned(), "x".to_owned())
            .await
            .unwrap_err();
        assert!(matches!(missing, AppError::NotFound(_)));
    }

    #[tokio::test]
    async fn delete_removes_only_that_snippet() {
        let (_dir, db) = open().await;
        let gone = db
            .add_snippet("gone".to_owned(), "x".to_owned())
            .await
            .unwrap();
        let kept = db
            .add_snippet("kept".to_owned(), "y".to_owned())
            .await
            .unwrap();
        db.delete_snippet(gone.id).await.unwrap();
        db.delete_snippet(gone.id).await.unwrap();
        assert_eq!(db.snippets().await.unwrap(), vec![kept]);
    }
}
