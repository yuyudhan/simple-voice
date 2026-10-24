// FilePath: crates/sv-text/examples/dictionary.rs
//! The dictation pipeline's dictionary steps for scripts/eval/transcription-eval.sh, so the eval
//! scores what the app would paste with formatting off, not only what the model heard.
//!
//!   dictionary boost-terms FILE   the names the app sends Parakeet TDT, one per line
//!   dictionary format FILE        each stdin line through the deterministic formatter
//!
//! FILE holds one dictionary entry per line, "heard -> written" for a rule, as the app imports.
#![forbid(unsafe_code)]

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use sv_domain::{DictionaryEntry, DictionarySource, Style};
use sv_text::Vocabulary;

fn entries(path: &str) -> io::Result<Vec<DictionaryEntry>> {
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

fn run(mode: &str, path: &str) -> io::Result<()> {
    let vocabulary = Vocabulary::new(&entries(path)?, &[]);
    let mut out = io::stdout().lock();
    match mode {
        "boost-terms" => {
            for term in vocabulary.boost_terms() {
                writeln!(out, "{term}")?;
            }
        }
        _ => {
            for line in io::stdin().lock().lines() {
                let formatted = sv_text::format(&line?, &vocabulary, Style::Formal);
                writeln!(out, "{}", formatted.text)?;
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(path)) = (args.first(), args.get(1)) else {
        eprintln!("usage: dictionary boost-terms|format FILE");
        return ExitCode::from(2);
    };
    if mode != "boost-terms" && mode != "format" {
        eprintln!("usage: dictionary boost-terms|format FILE");
        return ExitCode::from(2);
    }
    match run(mode, path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("dictionary: {error}");
            ExitCode::FAILURE
        }
    }
}
