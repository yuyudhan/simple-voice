// FilePath: crates/sv-storage/src/model_insights.rs
//! Per-model speed for the Insights "Models" tab. Only rows whose stage was timed count, so
//! dictations recorded before timings existed never drag an average toward zero.

use sv_domain::{AppError, AppResult, ModelInsights, ModelTiming};

use crate::db::Db;
use crate::history::display_name;

/// One aggregate row; both stages select the same columns so they share this shape.
#[derive(Debug)]
struct TimingRow {
    model: String,
    runs: i64,
    average_ms: f64,
    fastest_ms: i64,
    slowest_ms: i64,
    total_ms: i64,
    audio_ms: i64,
    words: i64,
}

impl From<TimingRow> for ModelTiming {
    fn from(row: TimingRow) -> Self {
        Self {
            name: display_name(&row.model),
            model: row.model,
            runs: row.runs,
            average_ms: row.average_ms,
            fastest_ms: row.fastest_ms,
            slowest_ms: row.slowest_ms,
            total_ms: row.total_ms,
            audio_ms: row.audio_ms,
            words: row.words,
        }
    }
}

impl Db {
    pub async fn model_insights(&self) -> AppResult<ModelInsights> {
        let inner = self.read().await;

        let transcription = sqlx::query_as!(
            TimingRow,
            r#"SELECT model AS "model!: String",
                      COUNT(*) AS "runs!: i64",
                      AVG(transcribe_ms) AS "average_ms!: f64",
                      MIN(transcribe_ms) AS "fastest_ms!: i64",
                      MAX(transcribe_ms) AS "slowest_ms!: i64",
                      SUM(transcribe_ms) AS "total_ms!: i64",
                      SUM(audio_ms) AS "audio_ms!: i64",
                      SUM(word_count) AS "words!: i64"
               FROM history WHERE transcribe_ms IS NOT NULL
               GROUP BY model ORDER BY 2 DESC, 1"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;

        let formatting = sqlx::query_as!(
            TimingRow,
            r#"SELECT format_model AS "model!: String",
                      COUNT(*) AS "runs!: i64",
                      AVG(format_ms) AS "average_ms!: f64",
                      MIN(format_ms) AS "fastest_ms!: i64",
                      MAX(format_ms) AS "slowest_ms!: i64",
                      SUM(format_ms) AS "total_ms!: i64",
                      SUM(audio_ms) AS "audio_ms!: i64",
                      SUM(word_count) AS "words!: i64"
               FROM history
               WHERE format_ms IS NOT NULL AND format_model IS NOT NULL
                 AND source_text IS NULL
               GROUP BY format_model ORDER BY 2 DESC, 1"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?;

        Ok(ModelInsights {
            transcription: transcription.into_iter().map(ModelTiming::from).collect(),
            formatting: formatting.into_iter().map(ModelTiming::from).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use sv_domain::{HistoryStatus, NewHistory, Style};

    use super::*;

    fn timed(model: &str, transcribe_ms: Option<i64>, format: Option<(&str, i64)>) -> NewHistory {
        NewHistory {
            created_at: 1_750_000_000_000,
            status: HistoryStatus::Pasted,
            raw_text: "ship it today".to_owned(),
            final_text: "Ship it today.".to_owned(),
            source_text: None,
            error: None,
            model: model.to_owned(),
            format_model: format.map(|(id, _)| id.to_owned()),
            language: Some("en".to_owned()),
            style: Style::Formal,
            audio_ms: 3_000,
            latency_ms: 900,
            transcribe_ms,
            format_ms: format.map(|(_, ms)| ms),
            dictionary_fixes: 0,
            app_name: None,
            bundle_id: None,
            audio_path: None,
        }
    }

    #[tokio::test]
    async fn aggregates_only_timed_runs_per_model() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        let rows = [
            timed("groq-whisper", Some(400), Some(("qwen/qwen3.8-27b", 1_000))),
            timed("groq-whisper", Some(600), Some(("qwen/qwen3.8-27b", 1_500))),
            timed("groq-whisper", Some(500), None),
            timed(
                "parakeet-tdt-v3",
                Some(300),
                Some(("apple-intelligence", 2_000)),
            ),
            // Recorded before timings existed: excluded from every aggregate.
            timed("parakeet-tdt-v3", None, None),
        ];
        for row in rows {
            db.insert_history(row).await.unwrap();
        }

        let insights = db.model_insights().await.unwrap();

        let whisper = &insights.transcription[0];
        assert_eq!(whisper.model, "groq-whisper");
        assert_eq!(whisper.name, "Groq Whisper");
        assert_eq!(whisper.runs, 3);
        assert!((whisper.average_ms - 500.0).abs() < f64::EPSILON);
        assert_eq!((whisper.fastest_ms, whisper.slowest_ms), (400, 600));
        assert_eq!((whisper.total_ms, whisper.audio_ms), (1_500, 9_000));
        let parakeet = &insights.transcription[1];
        assert_eq!((parakeet.runs, parakeet.total_ms), (1, 300));
        assert_eq!(insights.transcription.len(), 2);

        let qwen = &insights.formatting[0];
        assert_eq!(qwen.model, "qwen/qwen3.8-27b");
        assert_eq!(qwen.runs, 2);
        assert!((qwen.average_ms - 1_250.0).abs() < f64::EPSILON);
        assert_eq!(qwen.words, 6);
        assert_eq!(insights.formatting[1].name, "Apple Intelligence");
        assert_eq!(insights.formatting.len(), 2);
    }

    #[tokio::test]
    async fn empty_history_has_no_models() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        let insights = db.model_insights().await.unwrap();
        assert!(insights.transcription.is_empty());
        assert!(insights.formatting.is_empty());
    }
}
