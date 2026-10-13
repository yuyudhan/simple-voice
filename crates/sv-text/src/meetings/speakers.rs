// FilePath: crates/sv-text/src/meetings/speakers.rs
//! Giving each transcript segment the diarized speaker it overlaps most, timestamps, and the
//! transcript as the prompts read it.

use super::*;

/// One speaker's turn as the diarization model reported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerSpan {
    pub start_ms: i64,
    pub end_ms: i64,
    /// The model's own label ("S1").
    pub speaker: String,
}

/// A transcript segment with the index of its speaker; `None` when no turn overlaps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttributedSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub speaker: Option<usize>,
}

/// The transcript with speakers numbered `0..speaker_count` in order of first appearance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribution {
    pub segments: Vec<AttributedSegment>,
    pub speaker_count: usize,
}

/// Gives every segment the speaker whose turns overlap it longest (the earliest such speaker
/// on a tie). Speakers that end up owning no segment are not counted.
pub fn assign_speakers(mut segments: Vec<TimedText>, turns: &[SpeakerSpan]) -> Attribution {
    segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    let mut order: Vec<&str> = Vec::new();
    let mut totals: Vec<(&str, i64)> = Vec::new();
    let mut attributed = Vec::with_capacity(segments.len());
    for segment in segments {
        // A zero-length segment still sits at a point in time.
        let end = segment.end_ms.max(segment.start_ms + 1);
        totals.clear();
        for turn in turns {
            let overlap = end.min(turn.end_ms) - segment.start_ms.max(turn.start_ms);
            if overlap <= 0 {
                continue;
            }
            match totals
                .iter_mut()
                .find(|(speaker, _)| *speaker == turn.speaker)
            {
                Some((_, total)) => *total += overlap,
                None => totals.push((turn.speaker.as_str(), overlap)),
            }
        }
        let mut best: Option<(&str, i64)> = None;
        for &(speaker, total) in &totals {
            if best.is_none_or(|(_, longest)| total > longest) {
                best = Some((speaker, total));
            }
        }
        let speaker = best.map(|(speaker, _)| {
            order
                .iter()
                .position(|known| *known == speaker)
                .unwrap_or_else(|| {
                    order.push(speaker);
                    order.len() - 1
                })
        });
        attributed.push(AttributedSegment {
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            text: segment.text,
            speaker,
        });
    }
    Attribution {
        segments: attributed,
        speaker_count: order.len(),
    }
}

/// `mm:ss` under an hour, `h:mm:ss` from then on.
pub fn format_timestamp(ms: i64) -> String {
    let seconds = ms.max(0) / 1000;
    let (hours, minutes, seconds) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// Reads `mm:ss` or `h:mm:ss` back into milliseconds.
pub fn parse_timestamp(text: &str) -> Option<i64> {
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut numbers = Vec::with_capacity(parts.len());
    for part in &parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        numbers.push(part.parse::<i64>().ok()?);
    }
    let (hours, minutes, seconds) = match numbers.as_slice() {
        [minutes, seconds] => (0, *minutes, *seconds),
        [hours, minutes, seconds] if *minutes < 60 => (*hours, *minutes, *seconds),
        _ => return None,
    };
    if seconds >= 60 {
        return None;
    }
    Some(((hours * 60 + minutes) * 60 + seconds) * 1000)
}

/// One transcript segment as prompts and exports show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptLine<'a> {
    pub id: i64,
    pub start_ms: i64,
    /// The user's name for the speaker, else its label, else "Unknown speaker".
    pub speaker: &'a str,
    pub text: &'a str,
}

/// The transcript of `detail` in time order with speakers resolved to display names.
pub fn transcript_lines(detail: &MeetingDetail) -> Vec<TranscriptLine<'_>> {
    detail
        .segments
        .iter()
        .map(|segment| TranscriptLine {
            id: segment.id,
            start_ms: segment.start_ms,
            speaker: segment
                .speaker_id
                .and_then(|id| detail.speakers.iter().find(|speaker| speaker.id == id))
                .map_or(UNKNOWN_SPEAKER, |speaker| {
                    speaker.name.as_deref().unwrap_or(&speaker.label)
                }),
            text: segment.text.as_str(),
        })
        .collect()
}
