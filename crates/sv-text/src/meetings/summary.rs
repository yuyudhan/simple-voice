// FilePath: crates/sv-text/src/meetings/summary.rs
//! The map-reduce summary prompts and the strict parser of their replies.

use super::*;

/// `[mm:ss] Speaker: text`
fn summary_line(line: &TranscriptLine<'_>) -> String {
    format!(
        "[{}] {}: {}",
        format_timestamp(line.start_ms),
        line.speaker,
        one_line(clip(line.text, MAX_LINE_CHARS))
    )
}

/// Characters of transcript one summary request reads: Apple's on-device model has a 4,096
/// token context shared by prompt and reply.
pub fn summary_chunk_chars(target: PolishTarget) -> usize {
    match target {
        PolishTarget::OnDevice => 6_000,
        PolishTarget::Chat => 14_000,
    }
}

/// The transcript as consecutive chunks of at most `max_chars`, each line `[mm:ss] Speaker:
/// text`, never splitting a line.
pub fn summary_chunks(lines: &[TranscriptLine<'_>], max_chars: usize) -> Vec<String> {
    let rendered: Vec<String> = lines.iter().map(summary_line).collect();
    join_bounded(&rendered, "\n", max_chars)
}

/// Joins `items` into groups of at most `max_chars` (an item longer than that is its own
/// group), keeping their order.
pub fn join_bounded(items: &[String], separator: &str, max_chars: usize) -> Vec<String> {
    let mut groups = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0;
    for item in items {
        let chars = item.chars().count();
        let added = if current.is_empty() {
            chars
        } else {
            chars + separator.chars().count()
        };
        if !current.is_empty() && current_chars + added > max_chars {
            groups.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        if !current.is_empty() {
            current.push_str(separator);
            current_chars += separator.chars().count();
        }
        current.push_str(item);
        current_chars += chars;
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

const SUMMARY_FORMAT: &str = "Reply in exactly this format and nothing else:
SUMMARY:
<short paragraphs or bullet points: the topics, decisions and open questions>
ACTION ITEMS:
- <time> | <owner> | <task>

Write one line per action item. <time> is the [time] of the line where it was agreed, <owner> \
is the person responsible as named in the transcript, and `-` stands for an unknown time or \
owner. Write NONE under ACTION ITEMS when nobody agreed to do anything. Write in the \
language of the transcript.";

const SUMMARY_SHOT_USER: &str = "Part 1 of 1 of the meeting transcript:
<transcript>
[00:04] Speaker 1: Let's ship the beta on Friday if the login bug is fixed.
[00:11] Priya: I'll fix the login bug by Thursday.
[00:19] Speaker 1: Great, and we still need to pick a launch date for the full release.
</transcript>";

const SUMMARY_SHOT_REPLY: &str = "SUMMARY:
The beta ships on Friday provided the login bug is fixed. The launch date of the full release \
is still open.
ACTION ITEMS:
- 00:11 | Priya | Fix the login bug by Thursday";

/// Summarizes part `part` (1-based) of `parts` of a transcript chunk from [`summary_chunks`].
pub fn summary_prompt(chunk: &str, part: usize, parts: usize) -> PolishPrompt {
    let system = format!(
        "You summarize meeting transcripts. Every transcript line reads `[time] Speaker: \
         words`. Use only what the transcript says: never invent names, decisions, dates or \
         tasks.\n{SUMMARY_FORMAT}"
    );
    PolishPrompt {
        system,
        shots: vec![(SUMMARY_SHOT_USER.to_owned(), SUMMARY_SHOT_REPLY.to_owned())],
        user: format!(
            "Part {part} of {parts} of the meeting transcript:\n<transcript>\n{chunk}\n\
             </transcript>"
        ),
    }
}

/// Combines summaries of consecutive parts (rendered with [`render_summary`] and grouped with
/// [`join_bounded`]) into one.
pub fn summary_merge_prompt(partials: &str) -> PolishPrompt {
    let system = format!(
        "You combine summaries of consecutive parts of one meeting into a single summary of the \
         whole meeting. Merge repeated topics, keep every decision and every action item with \
         its time and owner, and add nothing the part summaries do not say.\n{SUMMARY_FORMAT}"
    );
    PolishPrompt {
        system,
        shots: Vec::new(),
        user: format!("Part summaries, in meeting order:\n<summaries>\n{partials}\n</summaries>"),
    }
}

/// A summary read from a model reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSummary {
    pub text: String,
    pub action_items: Vec<ActionItem>,
}

/// Writes a parsed summary back in the reply format, as input to [`summary_merge_prompt`].
pub fn render_summary(summary: &ParsedSummary) -> String {
    let mut text = format!("SUMMARY:\n{}\nACTION ITEMS:\n", summary.text);
    if summary.action_items.is_empty() {
        text.push_str("NONE");
    }
    for item in &summary.action_items {
        let time = item
            .start_ms
            .map_or_else(|| "-".to_owned(), format_timestamp);
        let owner = item.owner.as_deref().unwrap_or("-");
        let _ = writeln!(text, "- {time} | {owner} | {}", item.text);
    }
    text.trim_end().to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Preamble,
    Summary,
    Actions,
}

/// Reads a reply in the [`summary_prompt`] format. Anything that does not follow it is an
/// error, never a guessed summary. Errors never quote the reply, so they are safe to log.
pub fn parse_summary(reply: &str) -> Result<ParsedSummary, String> {
    let mut section = Section::Preamble;
    let mut summary: Vec<&str> = Vec::new();
    let mut items = Vec::new();
    let mut saw_actions = false;
    for raw in reply.lines() {
        let line = raw.trim();
        if let Some(rest) = heading(line, "SUMMARY:") {
            if section != Section::Preamble {
                return Err("The summary reply has more than one SUMMARY section.".to_owned());
            }
            section = Section::Summary;
            if !rest.is_empty() {
                summary.push(rest);
            }
            continue;
        }
        if let Some(rest) = heading(line, "ACTION ITEMS:") {
            if section != Section::Summary {
                return Err("The summary reply does not start with a SUMMARY section.".to_owned());
            }
            section = Section::Actions;
            saw_actions = true;
            if !rest.is_empty() && !is_none(rest) {
                return Err("The action items in the summary reply are malformed.".to_owned());
            }
            continue;
        }
        match section {
            // Chatter before the summary ("Here is the summary:") is not part of it.
            Section::Preamble => {}
            Section::Summary => summary.push(raw.trim_end()),
            Section::Actions => {
                if line.is_empty() || is_none(line) {
                    continue;
                }
                let item = ["- ", "* ", "• "]
                    .iter()
                    .find_map(|bullet| line.strip_prefix(bullet))
                    .ok_or_else(|| {
                        "An action item in the summary reply is not a `- time | owner | task` \
                         line."
                            .to_owned()
                    })?;
                items.push(parse_action_item(item)?);
            }
        }
    }
    if section == Section::Preamble {
        return Err("The reply has no SUMMARY section.".to_owned());
    }
    if !saw_actions {
        return Err("The summary reply has no ACTION ITEMS section; it may be cut off.".to_owned());
    }
    let text = summary.join("\n").trim().to_owned();
    if text.is_empty() {
        return Err("The summary reply is empty.".to_owned());
    }
    Ok(ParsedSummary {
        text,
        action_items: items,
    })
}

/// The rest of `line` after a heading such as `SUMMARY:`, tolerating Markdown emphasis.
fn heading<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let bare = line.trim_start_matches(['#', '*', ' ']);
    let head = bare.get(..name.len())?;
    if !head.eq_ignore_ascii_case(name) {
        return None;
    }
    Some(bare[name.len()..].trim_matches(['*', ' ']))
}

fn is_none(text: &str) -> bool {
    let word = text.trim().trim_end_matches('.');
    word.eq_ignore_ascii_case("none") || word == "-"
}

fn parse_action_item(line: &str) -> Result<ActionItem, String> {
    let malformed =
        || "An action item in the summary reply is not `time | owner | task`.".to_owned();
    let mut fields = line.splitn(3, '|');
    let (Some(time), Some(owner), Some(task)) = (fields.next(), fields.next(), fields.next())
    else {
        return Err(malformed());
    };
    let time = time
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let start_ms = if time.is_empty() || time == "-" {
        None
    } else {
        Some(parse_timestamp(time).ok_or_else(malformed)?)
    };
    let owner = owner.trim();
    let owner = ["-", "", "unknown", "none", "unassigned", "nobody"]
        .iter()
        .all(|blank| !owner.eq_ignore_ascii_case(blank))
        .then(|| owner.to_owned());
    let task = task.trim();
    if task.is_empty() {
        return Err(malformed());
    }
    Ok(ActionItem {
        text: task.to_owned(),
        owner,
        start_ms,
    })
}
