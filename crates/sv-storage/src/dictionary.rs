// FilePath: crates/sv-storage/src/dictionary.rs
//! Personal dictionary: plain words that bias recognition and `heard -> written` rules.

use sqlx::SqlitePool;
use sv_domain::{AppError, AppResult, DictionaryEntry, ImportSummary};

use crate::db::Db;

const MAX_PHRASE_CHARS: usize = 100;
const RULE_ARROW: &str = "->";

impl Db {
    /// Every entry, ordered by phrase (case-insensitive).
    pub async fn dictionary(&self) -> AppResult<Vec<DictionaryEntry>> {
        let inner = self.read().await;
        sqlx::query_as!(
            DictionaryEntry,
            r#"SELECT id AS "id!: i64", phrase AS "phrase!: String",
                      replacement AS "replacement?: String", created_at AS "created_at!: i64"
               FROM dictionary ORDER BY phrase COLLATE NOCASE, id"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)
    }

    pub async fn add_dictionary_entry(
        &self,
        phrase: String,
        replacement: Option<String>,
    ) -> AppResult<DictionaryEntry> {
        let (phrase, replacement) = clean(&phrase, replacement.as_deref())?;
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let id = sqlx::query!(
            "INSERT INTO dictionary (phrase, replacement, created_at) VALUES (?1, ?2, ?3)",
            phrase,
            replacement,
            now
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| duplicate_or_database(error, &phrase))?
        .last_insert_rowid();
        stored_entry(&inner.pool, id).await
    }

    pub async fn update_dictionary_entry(
        &self,
        id: i64,
        phrase: String,
        replacement: Option<String>,
    ) -> AppResult<DictionaryEntry> {
        let (phrase, replacement) = clean(&phrase, replacement.as_deref())?;
        let inner = self.read().await;
        let updated = sqlx::query!(
            "UPDATE dictionary SET phrase = ?1, replacement = ?2 WHERE id = ?3",
            phrase,
            replacement,
            id
        )
        .execute(&inner.pool)
        .await
        .map_err(|error| duplicate_or_database(error, &phrase))?;
        if updated.rows_affected() == 0 {
            return Err(AppError::NotFound("Dictionary entry".to_owned()));
        }
        stored_entry(&inner.pool, id).await
    }

    pub async fn delete_dictionary_entry(&self, id: i64) -> AppResult<()> {
        let inner = self.read().await;
        sqlx::query!("DELETE FROM dictionary WHERE id = ?1", id)
            .execute(&inner.pool)
            .await
            .map_err(AppError::database)?;
        Ok(())
    }

    /// Imports a vocabulary file: blank lines and `#` comments are skipped, `a -> b` lines become
    /// rules, anything else is a plain word. Phrases already present are counted as skipped.
    pub async fn import_vocabulary(&self, text: &str) -> AppResult<ImportSummary> {
        let entries: Vec<(String, Option<String>)> = text.lines().filter_map(parse_line).collect();
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let mut summary = ImportSummary {
            added: 0,
            skipped: 0,
        };
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        for (phrase, replacement) in entries {
            let inserted = sqlx::query!(
                "INSERT INTO dictionary (phrase, replacement, created_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT (phrase) DO NOTHING",
                phrase,
                replacement,
                now
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
            if inserted.rows_affected() == 0 {
                summary.skipped = summary.skipped.saturating_add(1);
            } else {
                summary.added = summary.added.saturating_add(1);
            }
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(summary)
    }
}

async fn stored_entry(pool: &SqlitePool, id: i64) -> AppResult<DictionaryEntry> {
    sqlx::query_as!(
        DictionaryEntry,
        r#"SELECT id AS "id!: i64", phrase AS "phrase!: String",
                  replacement AS "replacement?: String", created_at AS "created_at!: i64"
           FROM dictionary WHERE id = ?1"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::database)?
    .ok_or_else(|| AppError::NotFound("Dictionary entry".to_owned()))
}

/// Trims both sides; an empty replacement means a plain word.
fn clean(phrase: &str, replacement: Option<&str>) -> AppResult<(String, Option<String>)> {
    let phrase = phrase.trim();
    if phrase.is_empty() {
        return Err(AppError::invalid("Enter a word or phrase"));
    }
    if phrase.chars().count() > MAX_PHRASE_CHARS {
        return Err(AppError::invalid(format!(
            "Dictionary phrases can be at most {MAX_PHRASE_CHARS} characters"
        )));
    }
    let replacement = replacement
        .map(str::trim)
        .filter(|replacement| !replacement.is_empty())
        .map(str::to_owned);
    Ok((phrase.to_owned(), replacement))
}

/// One vocabulary line as `(phrase, replacement)`, or `None` for blank lines, comments and lines
/// that are not a valid entry.
fn parse_line(line: &str) -> Option<(String, Option<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (phrase, replacement) = match line.split_once(RULE_ARROW) {
        Some((heard, written)) => (heard, Some(written)),
        None => (line, None),
    };
    clean(phrase, replacement).ok()
}

fn duplicate_or_database(error: sqlx::Error, phrase: &str) -> AppError {
    match &error {
        sqlx::Error::Database(db_error) if db_error.is_unique_violation() => {
            AppError::InvalidInput(format!("“{phrase}” is already in your dictionary"))
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

    #[tokio::test]
    async fn entries_are_trimmed_and_duplicates_rejected_case_insensitively() {
        let (_dir, db) = open().await;
        let rule = db
            .add_dictionary_entry("  argo cd ".to_owned(), Some(" ArgoCD ".to_owned()))
            .await;
        let rule = rule.unwrap();
        assert_eq!(rule.phrase, "argo cd");
        assert_eq!(rule.replacement.as_deref(), Some("ArgoCD"));

        let word = db
            .add_dictionary_entry("Ghostty".to_owned(), Some("  ".to_owned()))
            .await;
        assert_eq!(word.unwrap().replacement, None);

        let duplicate = db
            .add_dictionary_entry("GHOSTTY".to_owned(), None)
            .await
            .unwrap_err();
        assert_eq!(
            duplicate,
            AppError::InvalidInput("“GHOSTTY” is already in your dictionary".to_owned())
        );
        let empty = db
            .add_dictionary_entry("   ".to_owned(), None)
            .await
            .unwrap_err();
        assert!(matches!(empty, AppError::InvalidInput(_)));
        let long = db
            .add_dictionary_entry("x".repeat(101), None)
            .await
            .unwrap_err();
        assert!(matches!(long, AppError::InvalidInput(_)));

        let phrases: Vec<String> = db
            .dictionary()
            .await
            .unwrap()
            .into_iter()
            .map(|entry| entry.phrase)
            .collect();
        assert_eq!(phrases, vec!["argo cd", "Ghostty"]);
    }

    #[tokio::test]
    async fn update_and_delete_entries() {
        let (_dir, db) = open().await;
        let kanata = db
            .add_dictionary_entry("kanata".to_owned(), None)
            .await
            .unwrap();
        let raycast = db
            .add_dictionary_entry("Raycast".to_owned(), None)
            .await
            .unwrap();

        let updated = db
            .update_dictionary_entry(kanata.id, "canatta".to_owned(), Some("kanata".to_owned()))
            .await
            .unwrap();
        assert_eq!(updated.phrase, "canatta");
        assert_eq!(updated.replacement.as_deref(), Some("kanata"));
        assert_eq!(updated.created_at, kanata.created_at);

        let clash = db
            .update_dictionary_entry(kanata.id, "raycast".to_owned(), None)
            .await;
        assert!(matches!(clash, Err(AppError::InvalidInput(_))));

        db.delete_dictionary_entry(raycast.id).await.unwrap();
        assert_eq!(db.dictionary().await.unwrap(), vec![updated]);
    }

    #[tokio::test]
    async fn import_parses_words_rules_and_comments() {
        let (_dir, db) = open().await;
        db.add_dictionary_entry("Claude".to_owned(), None)
            .await
            .unwrap();
        let file = "# Speech-to-text vocabulary\n\
                    \n\
                    # AI agents\n\
                    omp\n\
                    Oh my pi\n\
                    claude\n\
                    ghostty -> Ghostty\n\
                    argo cd->ArgoCD\n\
                    OMP\n\
                    -> nothing heard\n";
        let summary = db.import_vocabulary(file).await.unwrap();
        assert_eq!(
            summary,
            ImportSummary {
                added: 4,
                skipped: 2
            }
        );

        let entries = db.dictionary().await.unwrap();
        let pairs: Vec<(&str, Option<&str>)> = entries
            .iter()
            .map(|entry| (entry.phrase.as_str(), entry.replacement.as_deref()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("argo cd", Some("ArgoCD")),
                ("Claude", None),
                ("ghostty", Some("Ghostty")),
                ("Oh my pi", None),
                ("omp", None),
            ]
        );
    }
}
