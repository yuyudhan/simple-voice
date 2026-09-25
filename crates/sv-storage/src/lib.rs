// FilePath: crates/sv-storage/src/lib.rs
//! Local storage for Simple Voice: one SQLite database holding settings, dictation history and
//! the personal dictionary, upgraded in place by embedded forward-only migrations.
//!
//! Every statement goes through the sqlx query macros so it is checked against the schema at
//! compile time.

#![forbid(unsafe_code)]

mod db;
mod dictionary;
mod history;
mod insights;
pub mod paths;
mod settings;

pub use db::Db;
