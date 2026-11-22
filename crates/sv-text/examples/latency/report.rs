// FilePath: crates/sv-text/examples/latency/report.rs
//! The bench's TSV rows and its per-bucket summary. "Today" is the stop with the live transcript
//! on (the default); "live off" the same stop without the stream. A clip's length is the word
//! count of the rules-only text (the deterministic formatter's output for the whole recording),
//! which both stops start from. "Words moved" compares the speculative text with today's word by
//! word with case and punctuation ignored (a word added, dropped or changed); "tokens moved"
//! compares exact tokens, so it also counts punctuation and capitals. The same two numbers
//! between today's and the live-off text (one transcript, two whole passes) are the model's own
//! variation: the tolerance the speculative text is held to. "Edits vs rules" counts each stop's
//! word changes against the rules-only text, so a pass that drops part of the dictation shows up
//! on its own side.

use std::fmt::Write as _;
use std::path::Path;

use sv_domain::text_stats::{word_count, words_corrected};

use crate::run::{Speculated, Stop};
use crate::wav::SAMPLE_RATE;

/// Upper word counts of the length buckets; the last bucket has no limit.
const BUCKETS: [(usize, &str); 5] = [
    (12, "<=12 words"),
    (60, "13-60"),
    (120, "61-120"),
    (250, "121-250"),
    (usize::MAX, "251+"),
];

#[derive(Debug)]
pub(crate) struct Row {
    clip: String,
    audio_s: f64,
    words: usize,
    today: Stop,
    live_off_ms: u64,
    speculated: Speculated,
    moved_words: usize,
    moved_tokens: usize,
    noise_words: usize,
    noise_tokens: usize,
    today_edits: usize,
    speculative_edits: usize,
}

impl Row {
    pub(crate) fn new(
        clip: &str,
        samples: &[i16],
        today: Stop,
        live_off: &Stop,
        speculated: Speculated,
    ) -> Self {
        let length = u32::try_from(samples.len()).unwrap_or(u32::MAX);
        Self {
            clip: clip.to_owned(),
            audio_s: f64::from(length) / f64::from(SAMPLE_RATE),
            words: word_count(&speculated.whole),
            moved_words: words_corrected(&today.text, &speculated.stop.text),
            moved_tokens: token_distance(&today.text, &speculated.stop.text),
            noise_words: words_corrected(&today.text, &live_off.text),
            noise_tokens: token_distance(&today.text, &live_off.text),
            today_edits: words_corrected(&speculated.whole, &today.text),
            speculative_edits: words_corrected(&speculated.whole, &speculated.stop.text),
            live_off_ms: live_off.ms,
            today,
            speculated,
        }
    }

    pub(crate) fn progress(&self) -> String {
        format!(
            "{} {:.0}s {}w: today {} ms, speculative {} ms ({} of {} words reused) {}",
            self.clip,
            self.audio_s,
            self.words,
            self.today.ms,
            self.speculated.stop.ms,
            self.speculated.reused_words,
            self.words,
            self.speculated.stop.note
        )
    }

    fn bucket(&self) -> &'static str {
        BUCKETS
            .iter()
            .find(|(limit, _)| self.words <= *limit)
            .map_or("251+", |(_, name)| name)
    }
}

const HEADER: &str = "clip\tbucket\taudio_s\twords\ttoday_ms\tlive_off_ms\tspeculative_ms\t\
today_asr_ms\tspeculative_asr_ms\tsegments\treused\treused_words\ttail_words\trevised\tdrafts\t\
draft_ms\tpartials\tstream_failed\tmoved_words\tmoved_tokens\tnoise_words\tnoise_tokens\t\
today_edits\tspeculative_edits\ttoday_note\tspeculative_note\ttoday_text\tspeculative_text\t\
rules_text\theard\tlast_draft\n";

/// Writes every row so far to `out`, or stdout at the end when `out` is `None`.
pub(crate) fn write(out: Option<&Path>, rows: &[Row]) -> Result<(), String> {
    let mut tsv = HEADER.to_owned();
    for row in rows {
        let spec = &row.speculated;
        let _ = writeln!(
            tsv,
            "{}\t{}\t{:.1}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.clip,
            row.bucket(),
            row.audio_s,
            row.words,
            row.today.ms,
            row.live_off_ms,
            spec.stop.ms,
            row.today.asr_ms,
            spec.stop.asr_ms,
            spec.segments,
            spec.reused,
            spec.reused_words,
            spec.tail_words,
            spec.revised,
            spec.drafts,
            spec.draft_ms,
            spec.partials,
            spec.stream_failed,
            row.moved_words,
            row.moved_tokens,
            row.noise_words,
            row.noise_tokens,
            row.today_edits,
            row.speculative_edits,
            row.today.note,
            spec.stop.note,
            one_line(&row.today.text),
            one_line(&spec.stop.text),
            one_line(&spec.whole),
            one_line(&spec.heard),
            one_line(&spec.draft_text),
        );
    }
    match out {
        Some(path) => {
            std::fs::write(path, tsv).map_err(|error| format!("{}: {error}", path.display()))
        }
        None => {
            print!("{tsv}");
            Ok(())
        }
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\n', '\t'], " ⏎ ")
}

/// Median, p90 and the share figures per bucket, as a Markdown table.
pub(crate) fn summary(rows: &[Row]) -> String {
    let mut out = String::from(
        "| bucket | clips | words | today median | today p90 | live off median | speculative \
         median | speculative p90 | median change | p90 change | words reused | words moved | \
         tokens moved | model's own words moved | own tokens moved | today edits vs rules | \
         speculative edits vs rules |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (_, name) in BUCKETS {
        let bucket: Vec<&Row> = rows.iter().filter(|row| row.bucket() == name).collect();
        if bucket.is_empty() {
            continue;
        }
        let today: Vec<u64> = bucket.iter().map(|row| row.today.ms).collect();
        let live_off: Vec<u64> = bucket.iter().map(|row| row.live_off_ms).collect();
        let spec: Vec<u64> = bucket.iter().map(|row| row.speculated.stop.ms).collect();
        let words: usize = bucket.iter().map(|row| row.words).sum();
        let sum = |field: fn(&Row) -> usize| bucket.iter().map(|row| field(row)).sum::<usize>();
        let (today_median, spec_median) = (percentile(&today, 50), percentile(&spec, 50));
        let (today_p90, spec_p90) = (percentile(&today, 90), percentile(&spec, 90));
        let _ = writeln!(
            out,
            "| {name} | {} | {} | {today_median} ms | {today_p90} ms | {} ms | {spec_median} ms | \
             {spec_p90} ms | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            bucket.len(),
            words / bucket.len(),
            percentile(&live_off, 50),
            change(today_median, spec_median),
            change(today_p90, spec_p90),
            share(sum(|row| row.speculated.reused_words), words),
            share(sum(|row| row.moved_words), words),
            share(sum(|row| row.moved_tokens), words),
            share(sum(|row| row.noise_words), words),
            share(sum(|row| row.noise_tokens), words),
            share(sum(|row| row.today_edits), words),
            share(sum(|row| row.speculative_edits), words),
        );
    }
    out
}

/// Nearest-rank percentile.
fn percentile(values: &[u64], rank: usize) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() * rank).div_ceil(100).saturating_sub(1);
    sorted.get(index).copied().unwrap_or_default()
}

fn change(before: u64, after: u64) -> String {
    if before == 0 {
        return "-".to_owned();
    }
    let percent = (i128::from(after) - i128::from(before)) * 100 / i128::from(before);
    format!("{percent:+}%")
}

fn share(part: usize, whole: usize) -> String {
    if whole == 0 {
        return "-".to_owned();
    }
    format!("{}%", part * 100 / whole)
}

/// Word-level edit distance over exact whitespace tokens.
fn token_distance(before: &str, after: &str) -> usize {
    let a: Vec<&str> = before.split_whitespace().collect();
    let b: Vec<&str> = after.split_whitespace().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, word_a) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, word_b) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(word_a != word_b);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}
