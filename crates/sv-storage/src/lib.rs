// FilePath: crates/sv-storage/src/lib.rs
//! Local storage for Simple Voice: one SQLite database holding settings, dictation history,
//! the personal dictionary, snippets and meetings, upgraded in place by embedded forward-only migrations.
//!
//! Every statement goes through the sqlx query macros so it is checked against the schema at
//! compile time.

#![forbid(unsafe_code)]

mod db;
mod dictionary;
mod history;
mod insights;
mod meetings;
mod model_insights;
pub mod paths;
mod settings;
mod snippets;

pub use db::Db;
pub use meetings::NewSegment;
