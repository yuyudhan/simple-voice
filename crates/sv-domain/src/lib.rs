// FilePath: crates/sv-domain/src/lib.rs
//! Types shared by every Simple Voice crate and serialized to the UI.
//!
//! All structs serialize in camelCase to match `src/lib/api/types.ts`; all enums serialize in
//! snake_case. Nothing here performs I/O.
#![forbid(unsafe_code)]

pub mod apps;
pub mod dictation;
pub mod dictionary;
pub mod engine;
pub mod error;
pub mod history;
pub mod insights;
pub mod languages;
pub mod meetings;
pub mod models;
pub mod permissions;
pub mod quality;
pub mod settings;
pub mod snippets;
pub mod text_stats;
pub mod updates;
pub mod vocabulary_packs;

pub use apps::{categorize, AppCategory, KnownApp};
pub use dictation::{
    DictationPartial, DictationPhase, DictationState, ModelProgress, ModelProgressStatus,
};
pub use dictionary::{DictionaryEntry, DictionarySource, ImportSummary};
pub use engine::{EngineState, EngineStatus};
pub use error::{AppError, AppResult};
pub use history::{HistoryEntry, HistoryStatus, NewHistory, RetryReason};
pub use insights::{
    AppUsage, CategoryUsage, DayActivity, HourActivity, Insights, ModelInsights, ModelTiming,
    PersonalBests,
};
pub use languages::{Language, LanguageInfo, Script};
pub use meetings::{
    ActionItem, ChatMessage, ChatRole, Citation, ExportFormat, Meeting, MeetingDetail, MeetingJob,
    MeetingProgress, MeetingReadiness, MeetingState, MeetingStatus, MeetingSummary, Segment,
    Speaker, TranslatedSegment, Translation,
};
pub use models::{ModelInfo, ModelKind, ModelProvider, ModelStatus, SmartSelectPlan};
pub use permissions::{PermissionKind, PermissionStatus, Permissions};
pub use quality::TranscriptQuality;
pub use settings::{
    AppStyle, DictionarySort, LlmBackend, PostProcessing, Settings, SettingsPatch, SoundTheme,
    Style, StyleSource, Theme, TranscriptionProvider,
};
pub use snippets::Snippet;
pub use updates::{Release, UpdateStatus};
pub use vocabulary_packs::{VocabularyPack, VocabularyPackInfo};
