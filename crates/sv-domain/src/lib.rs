// FilePath: crates/sv-domain/src/lib.rs
//! Types shared by every Simple Voice crate and serialized to the UI.
//!
//! All structs serialize in camelCase to match `src/lib/api.ts`; all enums serialize in
//! snake_case. Nothing here performs I/O.
#![forbid(unsafe_code)]

pub mod apps;
pub mod dictation;
pub mod dictionary;
pub mod error;
pub mod history;
pub mod insights;
pub mod models;
pub mod permissions;
pub mod settings;
pub mod text_stats;
pub mod updates;

pub use apps::{categorize, AppCategory};
pub use dictation::{DictationPhase, DictationState, ModelProgress, ModelProgressStatus};
pub use dictionary::{DictionaryEntry, ImportSummary};
pub use error::{AppError, AppResult};
pub use history::{HistoryEntry, HistoryStatus, NewHistory};
pub use insights::{AppUsage, CategoryUsage, DayActivity, Insights};
pub use models::{ModelInfo, ModelKind, ModelProvider, ModelStatus};
pub use permissions::{PermissionKind, PermissionStatus, Permissions};
pub use settings::{PostProcessing, Settings, SettingsPatch, SoundTheme, Style, Theme};
pub use updates::{Release, UpdateStatus};
