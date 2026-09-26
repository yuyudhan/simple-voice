// FilePath: crates/sv-domain/src/insights.rs
//! Aggregates shown on the Insights screen.

use serde::{Deserialize, Serialize};

use crate::AppCategory;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayActivity {
    /// Local calendar date, `YYYY-MM-DD`.
    pub date: String,
    pub words: i64,
    pub dictations: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryUsage {
    pub category: AppCategory,
    pub dictations: i64,
    pub words: i64,
    /// 0..=100, share of dictations.
    pub percent: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUsage {
    pub name: String,
    pub bundle_id: Option<String>,
    pub words: i64,
}

/// Dictation totals for one local hour of the day, summed over all history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourActivity {
    /// 0..=23, local time.
    pub hour: i64,
    pub words: i64,
    pub dictations: i64,
}

/// All-time records. When two days tie, the earlier one holds the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalBests {
    /// Most words in a single dictation.
    pub longest_dictation_words: i64,
    /// Most words in one local day; the date is `YYYY-MM-DD`, `None` without history.
    pub best_day_words: i64,
    pub best_day_date: Option<String>,
    /// Most dictations in one local day.
    pub busiest_day_dictations: i64,
    pub busiest_day_date: Option<String>,
}

/// Only dictations that produced text (every status except `failed`) count.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Insights {
    pub total_words: i64,
    pub total_dictations: i64,
    /// Words / audio minutes; 0 when there is no audio.
    pub words_per_minute: f64,
    /// Minutes saved versus typing the same words at 40 wpm, minus speaking time; never negative.
    pub time_saved_minutes: f64,
    pub dictionary_fixes: i64,
    pub words_corrected: i64,
    /// Consecutive local days with at least one dictation, ending today (or yesterday).
    pub current_streak: i64,
    pub longest_streak: i64,
    pub words_this_month: i64,
    /// Percent change vs last calendar month; `None` when last month had no words.
    pub month_change_percent: Option<f64>,
    /// Last 26 weeks (182 days), oldest first, zero days included.
    pub days: Vec<DayActivity>,
    /// Sorted by dictations, descending.
    pub categories: Vec<CategoryUsage>,
    /// Top 5 apps by words.
    pub top_apps: Vec<AppUsage>,
    /// 24 entries, hour 0 first, zero hours included.
    pub hours: Vec<HourActivity>,
    pub bests: PersonalBests,
}

/// How one model performed over every dictation where its stage was timed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTiming {
    /// Model id as recorded, e.g. `groq-whisper` or `qwen/qwen3.8-27b`.
    pub model: String,
    pub name: String,
    pub runs: i64,
    pub average_ms: f64,
    pub fastest_ms: i64,
    pub slowest_ms: i64,
    /// Summed over the timed runs, so speed is total work over total time.
    pub total_ms: i64,
    pub audio_ms: i64,
    pub words: i64,
}

/// Per-model speed for the Insights "Models" tab, each list sorted by runs, descending.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInsights {
    pub transcription: Vec<ModelTiming>,
    pub formatting: Vec<ModelTiming>,
}
