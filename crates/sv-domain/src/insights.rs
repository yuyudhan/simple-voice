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
}
