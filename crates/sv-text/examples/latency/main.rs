// FilePath: crates/sv-text/examples/latency/main.rs
//! Stop-to-paste latency of formatting while speaking (requirements LT-5) against today's
//! whole-text pass, on real recordings, through the engine helper. Driven by
//! scripts/eval/latency-eval.sh.
//!
//!   latency compose --clips DIR --out DIR
//!       The bench clip set: every clip of DIR as is, plus composites of consecutive clips with
//!       a 0.5 s pause between them, ~30, ~88, ~183 and ~336 words long.
//!   latency run --clips DIR --engine PATH --models-dir DIR --asr MODEL --formatter MODEL
//!               [--dictionary FILE] [--style formal|casual] [--languages en] [--out FILE]
//!       Per clip: today's stop with the live transcript on (the clip streamed in real time for
//!       display, the default) and off (which also gives the model's own run-to-run variation
//!       on the same transcript), then the clip streamed in real time with the worker drafting
//!       and formatting while it plays, and the speculative stop. One TSV row per clip, then a
//!       summary per length bucket on stderr: median and p90 stop→text, reuse, and how far the
//!       text moved.
#![forbid(unsafe_code)]

mod engine;
mod report;
mod run;
mod wav;
mod worker;

#[path = "../common/entries.rs"]
mod common_entries;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use sv_domain::vocabulary_packs::default_vocabulary_packs;
use sv_domain::Style;
use sv_text::Vocabulary;

use crate::report::Row;
use crate::run::Setup;

/// Pause between the sentences of a composite: a speaker's breath between sentences.
const GAP_SAMPLES: usize = 8_000;
/// Composite lengths in words and how many of each.
const COMPOSITES: [(usize, usize); 4] = [(30, 5), (88, 5), (183, 4), (336, 3)];

fn usage() -> ExitCode {
    eprintln!(
        "usage: latency compose --clips DIR --out DIR\n       latency run --clips DIR \
         --engine PATH --models-dir DIR --asr MODEL --formatter MODEL [--dictionary FILE] \
         [--style formal|casual] [--languages en] [--out FILE]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((mode, rest)) = args.split_first() else {
        return usage();
    };
    let mut flags = HashMap::new();
    for pair in rest.chunks(2) {
        match pair {
            [flag, value] if flag.starts_with("--") => {
                flags.insert(flag.trim_start_matches("--").to_owned(), value.clone());
            }
            _ => return usage(),
        }
    }
    let result = match mode.as_str() {
        "compose" => compose(&flags),
        "run" => bench(&flags),
        _ => return usage(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("latency: {error}");
            ExitCode::FAILURE
        }
    }
}

fn flag<'a>(flags: &'a HashMap<String, String>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{name}"))
}

/// The labelled clips of `dir` (a `.wav` with its `.txt`), in name order.
fn clips(dir: &str) -> Result<Vec<(PathBuf, String)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("{dir}: {error}"))?;
    let mut clips: Vec<(PathBuf, String)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
        .filter_map(|wav| {
            let text = std::fs::read_to_string(wav.with_extension("txt")).ok()?;
            Some((wav, text.trim().to_owned()))
        })
        .collect();
    clips.sort();
    if clips.is_empty() {
        return Err(format!("no labelled .wav clips in {dir}"));
    }
    Ok(clips)
}

fn compose(flags: &HashMap<String, String>) -> Result<(), String> {
    let clips = clips(flag(flags, "clips")?)?;
    let out = Path::new(flag(flags, "out")?);
    std::fs::create_dir_all(out).map_err(|error| format!("{}: {error}", out.display()))?;
    let mut audio = Vec::with_capacity(clips.len());
    for (wav, text) in &clips {
        let samples = wav::read(wav)?;
        let name = wav
            .file_stem()
            .map(|stem| stem.to_string_lossy())
            .unwrap_or_default();
        write_clip(out, &format!("single-{name}"), &samples, text)?;
        audio.push((samples, text.clone()));
    }
    for (target, count) in COMPOSITES {
        for index in 0..count {
            let mut samples = Vec::new();
            let mut texts = Vec::new();
            let mut words = 0;
            let mut at = (index * 7 + target) % audio.len();
            while words < target {
                let Some((clip, text)) = audio.get(at) else {
                    break;
                };
                if !samples.is_empty() {
                    samples.resize(samples.len() + GAP_SAMPLES, 0);
                }
                samples.extend_from_slice(clip);
                texts.push(text.as_str());
                words += text.split_whitespace().count();
                at = (at + 1) % audio.len();
            }
            let name = format!("composite-{target:03}-{index}");
            write_clip(out, &name, &samples, &texts.join(" "))?;
        }
    }
    eprintln!("composed {} clips into {}", clips.len(), out.display());
    Ok(())
}

fn write_clip(dir: &Path, name: &str, samples: &[i16], text: &str) -> Result<(), String> {
    let wav = dir.join(format!("{name}.wav"));
    wav::write(&wav, samples)?;
    std::fs::write(wav.with_extension("txt"), format!("{text}\n"))
        .map_err(|error| format!("{}: {error}", wav.display()))
}

fn bench(flags: &HashMap<String, String>) -> Result<(), String> {
    let clips = clips(flag(flags, "clips")?)?;
    let entries = match flags.get("dictionary") {
        Some(path) => common_entries::entries(path).map_err(|error| format!("{path}: {error}"))?,
        None => Vec::new(),
    };
    let style = match flags.get("style").map(String::as_str) {
        None | Some("formal") => Style::Formal,
        Some("casual") => Style::Casual,
        Some(other) => return Err(format!("unknown style {other}")),
    };
    let languages = flags
        .get("languages")
        .map_or("en", String::as_str)
        .split(',')
        .filter(|language| !language.is_empty())
        .map(str::to_owned)
        .collect();
    let (engine, mut events) =
        engine::Engine::start(flag(flags, "engine")?, flag(flags, "models-dir")?)?;
    let setup = Arc::new(Setup {
        engine,
        asr: flag(flags, "asr")?.to_owned(),
        formatter: flag(flags, "formatter")?.to_owned(),
        vocabulary: Vocabulary::new(&entries, &default_vocabulary_packs()),
        style,
        languages,
    });
    for model in [&setup.asr, &setup.formatter] {
        let preload = serde_json::json!({ "model": model });
        setup
            .engine
            .call("preload", preload, std::time::Duration::from_secs(300))?;
    }
    let out = flags.get("out").map(PathBuf::from);
    if let Some((first, _)) = clips.first() {
        setup.warm_up(first)?;
    }
    let mut rows = Vec::new();
    for (wav, _) in &clips {
        let name = wav
            .file_stem()
            .map(|stem| stem.to_string_lossy())
            .unwrap_or_default();
        let samples = wav::read(wav)?;
        let today = setup.today_streamed(wav, &samples)?;
        let today_plain = setup.today(wav)?;
        let (speculated, returned) = worker::speculative(&setup, events, wav, &samples);
        events = returned;
        let speculated = speculated?;
        let row = Row::new(&name, &samples, today, &today_plain, speculated);
        eprintln!("{}", row.progress());
        rows.push(row);
        // Written after every clip, so an interrupted run keeps what it measured.
        if out.is_some() {
            report::write(out.as_deref(), &rows)?;
        }
    }
    setup.engine.stop();
    if out.is_none() {
        report::write(None, &rows)?;
    }
    eprint!("{}", report::summary(&rows));
    Ok(())
}
