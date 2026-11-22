// FilePath: crates/sv-text/examples/common/entries.rs
//! A personal dictionary file as the eval tools read it: one entry per line, "heard -> written"
//! for a rule, as Dictionary → Export writes it and the app imports it.

use std::io;

use sv_domain::{DictionaryEntry, DictionarySource};

pub(crate) fn entries(path: &str) -> io::Result<Vec<DictionaryEntry>> {
    let text = std::fs::read_to_string(path)?;
    let mut entries = Vec::new();
    for (id, line) in (1_i64..).zip(text.lines()) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (phrase, replacement) = match line.split_once("->") {
            Some((heard, written)) => (heard.trim(), Some(written.trim().to_owned())),
            None => (line, None),
        };
        entries.push(DictionaryEntry {
            id,
            phrase: phrase.to_owned(),
            replacement,
            created_at: 0,
            source: DictionarySource::Manual,
        });
    }
    Ok(entries)
}
