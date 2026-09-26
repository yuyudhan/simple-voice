// FilePath: crates/sv-storage/src/dictionary.rs
//! Personal dictionary: plain words that bias recognition and `heard -> written` rules. Words
//! learned from corrections are plain words marked `learned`; deleting one records it in
//! `dictionary_rejected` so it is never learned again.

use sqlx::{Executor, Sqlite};
use sv_domain::{AppError, AppResult, DictionaryEntry, DictionarySource, ImportSummary};

use crate::db::Db;

const MAX_PHRASE_CHARS: usize = 100;
const RULE_ARROW: &str = "->";

#[derive(Debug)]
struct DictionaryRow {
    id: i64,
    phrase: String,
    replacement: Option<String>,
    created_at: i64,
    source: String,
}

impl DictionaryRow {
    fn into_entry(self) -> DictionaryEntry {
        DictionaryEntry {
            id: self.id,
            phrase: self.phrase,
            replacement: self.replacement,
            created_at: self.created_at,
            source: DictionarySource::parse(&self.source),
        }
    }
}

impl Db {
    /// Every entry, ordered by phrase (case-insensitive).
    pub async fn dictionary(&self) -> AppResult<Vec<DictionaryEntry>> {
        let inner = self.read().await;
        let rows = sqlx::query_as!(
            DictionaryRow,
            r#"SELECT id AS "id!: i64", phrase AS "phrase!: String",
                      replacement AS "replacement?: String", created_at AS "created_at!: i64",
                      source AS "source!: String"
               FROM dictionary ORDER BY phrase COLLATE NOCASE, id"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;
        Ok(rows.into_iter().map(DictionaryRow::into_entry).collect())
    }

    pub async fn add_dictionary_entry(
        &self,
        phrase: String,
        replacement: Option<String>,
    ) -> AppResult<DictionaryEntry> {
        let (phrase, replacement) = clean(&phrase, replacement.as_deref())?;
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let source = DictionarySource::Manual.as_str();
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let id = sqlx::query!(
            "INSERT INTO dictionary (phrase, replacement, created_at, source)
             VALUES (?1, ?2, ?3, ?4)",
            phrase,
            replacement,
            now,
            source
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| duplicate_or_database(error, &phrase))?
        .last_insert_rowid();
        unreject(&mut *tx, &phrase).await?;
        let entry = stored_entry(&mut *tx, id).await?;
        tx.commit().await.map_err(AppError::database)?;
        Ok(entry)
    }

    /// An edited learned word becomes the user's own, so it turns `manual`.
    pub async fn update_dictionary_entry(
        &self,
        id: i64,
        phrase: String,
        replacement: Option<String>,
    ) -> AppResult<DictionaryEntry> {
        let (phrase, replacement) = clean(&phrase, replacement.as_deref())?;
        let inner = self.read().await;
        let source = DictionarySource::Manual.as_str();
        let updated = sqlx::query!(
            "UPDATE dictionary SET phrase = ?1, replacement = ?2, source = ?3 WHERE id = ?4",
            phrase,
            replacement,
            source,
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

    /// Deleting a learned word is the user saying it was wrong, so it is never learned again.
    pub async fn delete_dictionary_entry(&self, id: i64) -> AppResult<()> {
        let inner = self.read().await;
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        let row = sqlx::query!(
            r#"SELECT phrase AS "phrase!: String", source AS "source!: String"
               FROM dictionary WHERE id = ?1"#,
            id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::database)?;
        sqlx::query!("DELETE FROM dictionary WHERE id = ?1", id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        let learned =
            row.filter(|row| DictionarySource::parse(&row.source) == DictionarySource::Learned);
        if let Some(row) = learned {
            let now = chrono::Utc::now().timestamp_millis();
            sqlx::query!(
                "INSERT INTO dictionary_rejected (phrase, rejected_at) VALUES (?1, ?2)
                 ON CONFLICT (phrase) DO NOTHING",
                row.phrase,
                now
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(())
    }

    /// Imports a vocabulary file: blank lines and `#` comments are skipped, `a -> b` lines become
    /// rules, anything else is a plain word. Phrases already present are counted as skipped.
    pub async fn import_vocabulary(&self, text: &str) -> AppResult<ImportSummary> {
        let entries: Vec<(String, Option<String>)> = text.lines().filter_map(parse_line).collect();
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let source = DictionarySource::Manual.as_str();
        let mut summary = ImportSummary {
            added: 0,
            skipped: 0,
        };
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        for (phrase, replacement) in entries {
            let inserted = sqlx::query!(
                "INSERT INTO dictionary (phrase, replacement, created_at, source)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (phrase) DO NOTHING",
                phrase,
                replacement,
                now,
                source
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
            if inserted.rows_affected() == 0 {
                summary.skipped = summary.skipped.saturating_add(1);
            } else {
                summary.added = summary.added.saturating_add(1);
            }
            unreject(&mut *tx, &phrase).await?;
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(summary)
    }

    /// Adds words learned from corrections as plain `learned` words. Invalid phrases, phrases
    /// already in the dictionary and phrases the user rejected are skipped; returns the rows
    /// actually added.
    pub async fn learn_words(&self, phrases: &[String]) -> AppResult<Vec<DictionaryEntry>> {
        let inner = self.read().await;
        let now = chrono::Utc::now().timestamp_millis();
        let source = DictionarySource::Learned.as_str();
        let mut learned = Vec::new();
        let mut tx = inner.pool.begin().await.map_err(AppError::database)?;
        for phrase in phrases {
            let Ok((phrase, _)) = clean(phrase, None) else {
                continue;
            };
            let inserted = sqlx::query!(
                "INSERT INTO dictionary (phrase, created_at, source)
                 SELECT ?1, ?2, ?3
                 WHERE NOT EXISTS (SELECT 1 FROM dictionary_rejected WHERE phrase = ?1)
                 ON CONFLICT (phrase) DO NOTHING",
                phrase,
                now,
                source
            )
            .execute(&mut *tx)
            .await
            .map_err(AppError::database)?;
            if inserted.rows_affected() == 1 {
                learned.push(stored_entry(&mut *tx, inserted.last_insert_rowid()).await?);
            }
        }
        tx.commit().await.map_err(AppError::database)?;
        Ok(learned)
    }
}

async fn stored_entry<'e, E>(executor: E, id: i64) -> AppResult<DictionaryEntry>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query_as!(
        DictionaryRow,
        r#"SELECT id AS "id!: i64", phrase AS "phrase!: String",
                  replacement AS "replacement?: String", created_at AS "created_at!: i64",
                  source AS "source!: String"
           FROM dictionary WHERE id = ?1"#,
        id
    )
    .fetch_optional(executor)
    .await
    .map_err(AppError::database)?
    .map(DictionaryRow::into_entry)
    .ok_or_else(|| AppError::NotFound("Dictionary entry".to_owned()))
}

/// The user added the phrase themselves, so an earlier rejection of it no longer stands.
async fn unreject<'e, E>(executor: E, phrase: &str) -> AppResult<()>
where
    E: Executor<'e, Database = Sqlite>,
{
    sqlx::query!("DELETE FROM dictionary_rejected WHERE phrase = ?1", phrase)
        .execute(executor)
        .await
        .map_err(AppError::database)?;
    Ok(())
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

    fn phrases(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[tokio::test]
    async fn learn_words_adds_plain_learned_words_and_skips_known_and_invalid() {
        let (_dir, db) = open().await;
        let manual = db
            .add_dictionary_entry("Ghostty".to_owned(), None)
            .await
            .unwrap();
        assert_eq!(manual.source, DictionarySource::Manual);

        let long = "x".repeat(101);
        let batch = [
            " Wispr Flow ",
            "ghostty",
            "  ",
            long.as_str(),
            "wispr flow",
            "Tauri",
        ];
        let learned = db.learn_words(&phrases(&batch)).await.unwrap();
        let pairs: Vec<(&str, Option<&str>, DictionarySource)> = learned
            .iter()
            .map(|entry| {
                (
                    entry.phrase.as_str(),
                    entry.replacement.as_deref(),
                    entry.source,
                )
            })
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("Wispr Flow", None, DictionarySource::Learned),
                ("Tauri", None, DictionarySource::Learned),
            ]
        );

        let stored = db.dictionary().await.unwrap();
        assert_eq!(stored.len(), 3);
        assert!(stored.contains(&manual));
        assert!(learned.iter().all(|entry| stored.contains(entry)));
    }

    #[tokio::test]
    async fn deleting_a_learned_word_blocks_relearning_but_a_manual_word_does_not() {
        let (_dir, db) = open().await;
        let learned = db.learn_words(&phrases(&["Wispr"])).await.unwrap();
        db.delete_dictionary_entry(learned[0].id).await.unwrap();
        assert!(db
            .learn_words(&phrases(&["WISPR"]))
            .await
            .unwrap()
            .is_empty());
        assert!(db.dictionary().await.unwrap().is_empty());

        let manual = db
            .add_dictionary_entry("kanata".to_owned(), None)
            .await
            .unwrap();
        db.delete_dictionary_entry(manual.id).await.unwrap();
        let relearned = db.learn_words(&phrases(&["kanata"])).await.unwrap();
        assert_eq!(relearned.len(), 1);
        assert_eq!(relearned[0].source, DictionarySource::Learned);
    }

    #[tokio::test]
    async fn adding_or_importing_a_rejected_word_clears_the_rejection() {
        let (_dir, db) = open().await;
        let learned = db
            .learn_words(&phrases(&["Wispr", "Raycast"]))
            .await
            .unwrap();
        for entry in &learned {
            db.delete_dictionary_entry(entry.id).await.unwrap();
        }

        let added = db
            .add_dictionary_entry("wispr".to_owned(), None)
            .await
            .unwrap();
        db.import_vocabulary("RAYCAST\n").await.unwrap();
        let imported = db.dictionary().await.unwrap();
        assert!(imported
            .iter()
            .all(|entry| entry.source == DictionarySource::Manual));

        db.delete_dictionary_entry(added.id).await.unwrap();
        for entry in imported.iter().filter(|entry| entry.id != added.id) {
            db.delete_dictionary_entry(entry.id).await.unwrap();
        }
        let relearned = db
            .learn_words(&phrases(&["Wispr", "Raycast"]))
            .await
            .unwrap();
        assert_eq!(relearned.len(), 2);
    }

    #[tokio::test]
    async fn editing_a_learned_word_makes_it_manual() {
        let (_dir, db) = open().await;
        let learned = db.learn_words(&phrases(&["Wispr"])).await.unwrap();
        let edited = db
            .update_dictionary_entry(learned[0].id, "Wispr Flow".to_owned(), None)
            .await
            .unwrap();
        assert_eq!(edited.source, DictionarySource::Manual);

        db.delete_dictionary_entry(edited.id).await.unwrap();
        assert_eq!(
            db.learn_words(&phrases(&["Wispr Flow"]))
                .await
                .unwrap()
                .len(),
            1
        );
    }
}
