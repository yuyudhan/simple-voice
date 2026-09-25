// FilePath: crates/sv-storage/src/insights.rs
//! Aggregates for the Insights screen. SQLite does the heavy grouping (one row per active local
//! day, category and app), so the cost stays flat as history grows.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate};
use sv_domain::{AppCategory, AppError, AppResult, AppUsage, CategoryUsage, DayActivity, Insights};

use crate::db::Db;

const MS_PER_DAY: i64 = 86_400_000;
const CHART_DAYS: i64 = 182;
const TOP_APPS: i64 = 5;
const TYPING_WPM: f64 = 40.0;

#[derive(Debug, Clone, Copy)]
struct DayTotals {
    /// Days since 1970-01-01 in the user's local time.
    day: i64,
    dictations: i64,
    words: i64,
}

impl Db {
    /// `now_ms` and `utc_offset_minutes` come from the caller so "today" matches the user's clock.
    pub async fn insights(&self, now_ms: i64, utc_offset_minutes: i32) -> AppResult<Insights> {
        let inner = self.read().await;
        let offset_ms = i64::from(utc_offset_minutes) * 60_000;

        let totals = sqlx::query!(
            r#"SELECT COUNT(*) AS "dictations!: i64",
                      COALESCE(SUM(word_count), 0) AS "words!: i64",
                      COALESCE(SUM(audio_ms), 0) AS "audio_ms!: i64",
                      COALESCE(SUM(dictionary_fixes), 0) AS "dictionary_fixes!: i64",
                      COALESCE(SUM(words_corrected), 0) AS "words_corrected!: i64"
               FROM history WHERE status IN ('pasted', 'unformatted', 'dropped')"#
        )
        .fetch_one(&inner.pool)
        .await
        .map_err(AppError::database)?;

        let days: Vec<DayTotals> = sqlx::query!(
            r#"SELECT (created_at + ?1) / 86400000 AS "day!: i64",
                      COUNT(*) AS "dictations!: i64",
                      COALESCE(SUM(word_count), 0) AS "words!: i64"
               FROM history WHERE status IN ('pasted', 'unformatted', 'dropped')
               GROUP BY 1 ORDER BY 1"#,
            offset_ms
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?
        .into_iter()
        .map(|row| DayTotals {
            day: row.day,
            dictations: row.dictations,
            words: row.words,
        })
        .collect();

        let categories = sqlx::query!(
            r#"SELECT app_category AS "category!: String",
                      COUNT(*) AS "dictations!: i64",
                      COALESCE(SUM(word_count), 0) AS "words!: i64"
               FROM history WHERE status IN ('pasted', 'unformatted', 'dropped')
               GROUP BY app_category ORDER BY 2 DESC, 3 DESC"#
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?
        .into_iter()
        .map(|row| CategoryUsage {
            category: AppCategory::parse(&row.category),
            dictations: row.dictations,
            words: row.words,
            percent: percent_of(row.dictations, totals.dictations),
        })
        .collect();

        let top_apps = sqlx::query!(
            r#"SELECT app_name AS "name!: String",
                      MAX(bundle_id) AS "bundle_id?: String",
                      COALESCE(SUM(word_count), 0) AS "words!: i64"
               FROM history
               WHERE status IN ('pasted', 'unformatted', 'dropped') AND app_name IS NOT NULL
               GROUP BY app_name ORDER BY 3 DESC, 1 LIMIT ?1"#,
            TOP_APPS
        )
        .fetch_all(&inner.pool)
        .await
        .map_err(AppError::database)?
        .into_iter()
        .map(|row| AppUsage {
            name: row.name,
            bundle_id: row.bundle_id,
            words: row.words,
        })
        .collect();

        let today = (now_ms + offset_ms).div_euclid(MS_PER_DAY);
        let (current_streak, longest_streak) = streaks(&days, today);
        let (words_this_month, month_change_percent) = month_words(&days, today);
        let audio_minutes = totals.audio_ms as f64 / 60_000.0;
        let words = totals.words as f64;

        Ok(Insights {
            total_words: totals.words,
            total_dictations: totals.dictations,
            words_per_minute: if audio_minutes > 0.0 {
                words / audio_minutes
            } else {
                0.0
            },
            time_saved_minutes: (words / TYPING_WPM - audio_minutes).max(0.0),
            dictionary_fixes: totals.dictionary_fixes,
            words_corrected: totals.words_corrected,
            current_streak,
            longest_streak,
            words_this_month,
            month_change_percent,
            days: chart_days(&days, today)?,
            categories,
            top_apps,
        })
    }
}

fn percent_of(part: i64, whole: i64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 * 100.0 / whole as f64
    }
}

fn date_of(day: i64) -> Option<NaiveDate> {
    DateTime::from_timestamp(day.checked_mul(86_400)?, 0).map(|moment| moment.date_naive())
}

/// `(current, longest)` over active days sorted ascending. The current streak survives until the
/// end of the day after the last dictation, so it does not read 0 every morning.
fn streaks(days: &[DayTotals], today: i64) -> (i64, i64) {
    let mut longest = 0;
    let mut run = 0;
    let mut previous: Option<i64> = None;
    for day in days
        .iter()
        .map(|totals| totals.day)
        .filter(|day| *day <= today)
    {
        run = if previous == Some(day - 1) {
            run + 1
        } else {
            1
        };
        longest = longest.max(run);
        previous = Some(day);
    }
    let current = match previous {
        Some(last) if last >= today - 1 => run,
        _ => 0,
    };
    (current, longest)
}

/// Words in the current local calendar month and the percent change from the previous month.
fn month_words(days: &[DayTotals], today: i64) -> (i64, Option<f64>) {
    let Some(today) = date_of(today) else {
        return (0, None);
    };
    let this_month = (today.year(), today.month());
    let last_month = if today.month() == 1 {
        (today.year() - 1, 12)
    } else {
        (today.year(), today.month() - 1)
    };
    let mut this_words = 0;
    let mut last_words = 0;
    for totals in days {
        let Some(date) = date_of(totals.day) else {
            continue;
        };
        let month = (date.year(), date.month());
        if month == this_month {
            this_words += totals.words;
        } else if month == last_month {
            last_words += totals.words;
        }
    }
    let change =
        (last_words > 0).then(|| (this_words - last_words) as f64 * 100.0 / last_words as f64);
    (this_words, change)
}

/// The last 182 local days ending today, oldest first, with inactive days as zeros.
fn chart_days(days: &[DayTotals], today: i64) -> AppResult<Vec<DayActivity>> {
    let first = today - (CHART_DAYS - 1);
    let active: HashMap<i64, DayTotals> = days
        .iter()
        .filter(|totals| (first..=today).contains(&totals.day))
        .map(|totals| (totals.day, *totals))
        .collect();
    (first..=today)
        .map(|day| {
            let date = date_of(day)
                .ok_or_else(|| AppError::Other(format!("Day {day} is outside the calendar")))?;
            let totals = active.get(&day);
            Ok(DayActivity {
                date: date.format("%Y-%m-%d").to_string(),
                words: totals.map_or(0, |totals| totals.words),
                dictations: totals.map_or(0, |totals| totals.dictations),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use sv_domain::{HistoryStatus, NewHistory, Style};

    use super::*;

    const HOUR: i64 = 3_600_000;
    /// 2026-03-15 12:00:00 UTC.
    const NOW: i64 = 1_773_576_000_000;

    fn dictation(created_at: i64, text: &str, status: HistoryStatus) -> NewHistory {
        NewHistory {
            created_at,
            status,
            raw_text: text.to_owned(),
            final_text: text.to_owned(),
            error: None,
            model: "groq-whisper".to_owned(),
            language: Some("en".to_owned()),
            style: Style::Formal,
            audio_ms: 30_000,
            latency_ms: 250,
            dictionary_fixes: 1,
            app_name: Some("Slack".to_owned()),
            bundle_id: Some("com.tinyspeck.slackmacgap".to_owned()),
            audio_path: None,
        }
    }

    async fn open() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        (dir, db)
    }

    #[tokio::test]
    async fn empty_history_has_zero_filled_days() {
        let (_dir, db) = open().await;
        let insights = db.insights(NOW, 0).await.unwrap();
        assert_eq!(insights.total_dictations, 0);
        assert!(insights.words_per_minute.abs() < f64::EPSILON);
        assert_eq!(insights.month_change_percent, None);
        assert_eq!(insights.days.len(), 182);
        assert_eq!(insights.days.last().unwrap().date, "2026-03-15");
        assert_eq!(insights.days.first().unwrap().date, "2025-09-15");
        assert!(insights
            .days
            .iter()
            .all(|day| day.words == 0 && day.dictations == 0));
        assert!(insights.categories.is_empty() && insights.top_apps.is_empty());
    }

    #[tokio::test]
    async fn aggregates_streaks_months_and_apps() {
        let (_dir, db) = open().await;
        let day = 24 * HOUR;
        // Current streak: yesterday and the two days before (none today yet).
        for back in 1..=3 {
            let pasted = dictation(
                NOW - back * day,
                "one two three four",
                HistoryStatus::Pasted,
            );
            db.insert_history(pasted).await.unwrap();
        }
        // Longest streak: four consecutive days in February.
        for back in 20..=23 {
            db.insert_history(dictation(
                NOW - back * day,
                "one two",
                HistoryStatus::Unformatted,
            ))
            .await
            .unwrap();
        }
        // Failed dictations never count.
        db.insert_history(dictation(NOW, "ignored words here", HistoryStatus::Failed))
            .await
            .unwrap();
        db.insert_history(NewHistory {
            app_name: Some("Mail".to_owned()),
            bundle_id: Some("com.apple.mail".to_owned()),
            ..dictation(
                NOW - 3 * day,
                "five six seven eight nine ten",
                HistoryStatus::Dropped,
            )
        })
        .await
        .unwrap();

        let insights = db.insights(NOW, 0).await.unwrap();
        assert_eq!(insights.total_dictations, 8);
        assert_eq!(insights.total_words, 3 * 4 + 4 * 2 + 6);
        assert_eq!(insights.dictionary_fixes, 8);
        assert_eq!(insights.current_streak, 3);
        assert_eq!(insights.longest_streak, 4);
        // 26 words over 4 audio minutes.
        assert!((insights.words_per_minute - 6.5).abs() < 1e-9);
        assert!(insights.time_saved_minutes.abs() < f64::EPSILON);
        assert_eq!(insights.words_this_month, 12 + 6);
        assert!((insights.month_change_percent.unwrap() - 125.0).abs() < 1e-9);

        let recent = &insights.days[insights.days.len() - 4..];
        let words: Vec<i64> = recent.iter().map(|day| day.words).collect();
        assert_eq!(words, vec![10, 4, 4, 0]);
        assert_eq!(recent[0].dictations, 2);

        assert_eq!(insights.categories[0].category, AppCategory::WorkMessages);
        assert_eq!(insights.categories[0].dictations, 7);
        assert!((insights.categories[0].percent - 87.5).abs() < 1e-9);
        assert_eq!(insights.categories[1].category, AppCategory::Email);
        let apps: Vec<(&str, i64)> = insights
            .top_apps
            .iter()
            .map(|app| (app.name.as_str(), app.words))
            .collect();
        assert_eq!(apps, vec![("Slack", 20), ("Mail", 6)]);
    }

    #[tokio::test]
    async fn days_follow_the_local_utc_offset() {
        let (_dir, db) = open().await;
        // 2026-03-14 20:00 UTC is already 2026-03-15 in India (UTC+5:30).
        db.insert_history(dictation(
            NOW - 16 * HOUR,
            "late night",
            HistoryStatus::Pasted,
        ))
        .await
        .unwrap();

        let utc = db.insights(NOW, 0).await.unwrap();
        assert_eq!(utc.days.last().unwrap().words, 0);
        assert_eq!(utc.current_streak, 1);

        let india = db.insights(NOW, 330).await.unwrap();
        assert_eq!(india.days.last().unwrap().date, "2026-03-15");
        assert_eq!(india.days.last().unwrap().words, 2);
        assert_eq!(india.current_streak, 1);
    }

    #[test]
    fn streak_breaks_after_a_missed_day() {
        let days = [
            DayTotals {
                day: 100,
                dictations: 1,
                words: 1,
            },
            DayTotals {
                day: 101,
                dictations: 1,
                words: 1,
            },
        ];
        assert_eq!(streaks(&days, 101), (2, 2));
        assert_eq!(streaks(&days, 102), (2, 2));
        assert_eq!(streaks(&days, 103), (0, 2));
    }
}
