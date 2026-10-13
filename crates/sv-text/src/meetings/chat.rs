// FilePath: crates/sv-text/src/meetings/chat.rs
//! Transcript-grounded answers: retrieving bounded excerpts for a question, the prompt, and
//! the parser that keeps only citations of those excerpts.

use super::*;

/// Characters of transcript excerpts one question may use.
pub fn chat_excerpt_chars(target: PolishTarget) -> usize {
    match target {
        PolishTarget::OnDevice => 5_000,
        PolishTarget::Chat => 12_000,
    }
}

/// The transcript lines a question is answered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpts {
    /// `[#id mm:ss Speaker] text` lines in meeting order.
    pub text: String,
    /// Every segment in `text`, the only ones an answer may cite.
    pub segments: Vec<Citation>,
}

/// Picks the transcript lines that best match `question` (by shared words, rarer words
/// weighing more, each with its neighbours for context) until `max_chars` is used. A question
/// sharing no word with the transcript ("what happened?") gets lines spread evenly over the
/// meeting instead.
pub fn retrieve_excerpts(
    lines: &[TranscriptLine<'_>],
    question: &str,
    max_chars: usize,
) -> Excerpts {
    let terms = query_terms(question);
    let scores = line_scores(lines, &terms);
    let mut centres: Vec<usize> = (0..lines.len()).filter(|&i| scores[i] > 0.0).collect();
    centres.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
    if centres.is_empty() && !lines.is_empty() {
        let groups = MAX_EXCERPT_GROUPS.min(lines.len());
        centres = (0..groups)
            .map(|group| (group * lines.len() + lines.len() / 2) / groups)
            .collect();
    }

    let mut rendered: Vec<Option<String>> = vec![None; lines.len()];
    let mut used = 0;
    let mut full = false;
    for &centre in centres.iter().take(MAX_EXCERPT_GROUPS) {
        let from = centre.saturating_sub(CONTEXT_RADIUS);
        let to = (centre + CONTEXT_RADIUS).min(lines.len() - 1);
        // The matching line first, then its context.
        let order = std::iter::once(centre).chain((from..=to).filter(|&i| i != centre));
        for index in order {
            if rendered[index].is_some() {
                continue;
            }
            let line = excerpt_line(&lines[index]);
            let chars = line.chars().count() + 1;
            if used + chars > max_chars {
                full = true;
                break;
            }
            used += chars;
            rendered[index] = Some(line);
        }
        if full {
            break;
        }
    }

    let mut text = String::with_capacity(used);
    let mut segments = Vec::new();
    for (line, rendered) in lines.iter().zip(rendered) {
        if let Some(rendered) = rendered {
            text.push_str(&rendered);
            text.push('\n');
            segments.push(Citation {
                segment_id: line.id,
                start_ms: line.start_ms,
            });
        }
    }
    Excerpts {
        text: text.trim_end().to_owned(),
        segments,
    }
}

/// `[#id mm:ss Speaker] text`
fn excerpt_line(line: &TranscriptLine<'_>) -> String {
    format!(
        "[#{} {} {}] {}",
        line.id,
        format_timestamp(line.start_ms),
        line.speaker,
        one_line(clip(line.text, MAX_LINE_CHARS))
    )
}

fn query_terms(question: &str) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    for token in tokens(question) {
        if token.chars().count() < 2 || STOPWORDS.contains(&token.as_str()) {
            continue;
        }
        if !terms.contains(&token) {
            terms.push(token);
        }
        if terms.len() == MAX_QUERY_TERMS {
            break;
        }
    }
    terms
}

/// A word matches a term exactly or, for terms of four letters or more, as its prefix
/// ("deploy" matches "deployment").
fn matches_term(word: &str, term: &str) -> bool {
    word == term || (term.chars().count() >= 4 && word.starts_with(term))
}

/// Sum of the inverse document frequencies of the terms each line contains.
fn line_scores(lines: &[TranscriptLine<'_>], terms: &[String]) -> Vec<f64> {
    let present: Vec<Vec<usize>> = lines
        .iter()
        .map(|line| {
            let words: Vec<String> = tokens(line.text).collect();
            (0..terms.len())
                .filter(|&term| words.iter().any(|word| matches_term(word, &terms[term])))
                .collect()
        })
        .collect();
    let mut frequency = vec![0usize; terms.len()];
    for line in &present {
        for &term in line {
            frequency[term] += 1;
        }
    }
    let total = lines.len() as f64 + 1.0;
    present
        .iter()
        .map(|line| {
            line.iter()
                .map(|&term| (total / (frequency[term] as f64 + 1.0)).ln() + 1.0)
                .sum()
        })
        .collect()
}

const CHAT_SYSTEM: &str = "You answer questions about one meeting using only the transcript \
excerpts given. Every excerpt line starts with `[#id time speaker]`.
- Use only the excerpts, never outside knowledge or guesses.
- Cite the line every statement comes from by its id in square brackets, for example [#12] or \
[#12, #15].
- If the excerpts do not contain the answer, say that it is not in the transcript.
- Be brief. Answer in the language of the question.";

/// The grounded question prompt over `excerpts` from [`retrieve_excerpts`].
pub fn chat_prompt(excerpts: &Excerpts, question: &str) -> PolishPrompt {
    PolishPrompt {
        system: CHAT_SYSTEM.to_owned(),
        shots: Vec::new(),
        user: format!(
            "Transcript excerpts:\n<excerpts>\n{}\n</excerpts>\n\nQuestion: {}",
            excerpts.text,
            one_line(question)
        ),
    }
}

/// An answer with the excerpt segments it cites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatAnswer {
    pub text: String,
    pub citations: Vec<Citation>,
}

/// Reads an answer: citation groups such as `[#12, #15]` keep only ids that were among the
/// excerpts (a group citing none of them is removed from the text), and the citations come back
/// in order of first mention. Segment ids are internal, so the text shows each kept group as
/// the 1-based positions of its citations in that list (`[1, 2]`), which the UI links to the
/// cited moments.
pub fn parse_chat_answer(reply: &str, excerpts: &Excerpts) -> Result<ChatAnswer, String> {
    let mut text = String::with_capacity(reply.len());
    let mut citations: Vec<Citation> = Vec::new();
    let mut rest = reply;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some((ids, close)) = after
            .find(']')
            .and_then(|close| citation_ids(&after[..close]).map(|ids| (ids, close)))
        else {
            text.push_str(&rest[..=open]);
            rest = after;
            continue;
        };
        text.push_str(&rest[..open]);
        let known: Vec<Citation> = ids
            .into_iter()
            .filter_map(|id| {
                excerpts
                    .segments
                    .iter()
                    .find(|segment| segment.segment_id == id)
                    .copied()
            })
            .collect();
        if known.is_empty() {
            // A hallucinated reference: drop it, and the space before it.
            let trimmed = text.trim_end().len();
            text.truncate(trimmed);
        } else {
            let mut group: Vec<String> = Vec::with_capacity(known.len());
            for citation in known {
                let position = match citations.iter().position(|kept| *kept == citation) {
                    Some(position) => position,
                    None => {
                        citations.push(citation);
                        citations.len() - 1
                    }
                };
                let label = (position + 1).to_string();
                if !group.contains(&label) {
                    group.push(label);
                }
            }
            let _ = write!(text, "[{}]", group.join(", "));
        }
        rest = &after[close + 1..];
    }
    text.push_str(rest);
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("The model returned an empty answer.".to_owned());
    }
    Ok(ChatAnswer { text, citations })
}

/// The ids of a citation group's inside (`#12, #15`), or `None` when it is not one.
fn citation_ids(inside: &str) -> Option<Vec<i64>> {
    let mut ids = Vec::new();
    for part in inside
        .split([',', ' ', ';'])
        .filter(|part| !part.is_empty())
    {
        let digits = part.strip_prefix('#')?;
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        ids.push(digits.parse().ok()?);
    }
    (!ids.is_empty()).then_some(ids)
}
