// FilePath: crates/sv-text/src/meetings/windows.rs
//! Cutting a long recording into windows a speech model can transcribe, and stitching the
//! window transcripts back together without repeating the overlap.

use super::*;

/// A stretch of the recording sent to the speech model in one request. Segments whose
/// midpoint falls in `keep_from_ms..keep_until_ms` belong to this window; the rest of the
/// overlap belongs to its neighbour, so nothing is transcribed twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub keep_from_ms: i64,
    pub keep_until_ms: i64,
}

/// Covers `0..duration_ms` with windows of [`WINDOW_MS`] overlapping by [`WINDOW_OVERLAP_MS`].
pub fn audio_windows(duration_ms: i64) -> Vec<AudioWindow> {
    let mut windows = Vec::new();
    if duration_ms <= 0 {
        return windows;
    }
    let half = WINDOW_OVERLAP_MS / 2;
    let mut start = 0;
    loop {
        let end = (start + WINDOW_MS).min(duration_ms);
        let last = end >= duration_ms;
        windows.push(AudioWindow {
            start_ms: start,
            end_ms: end,
            keep_from_ms: if start == 0 { 0 } else { start + half },
            keep_until_ms: if last { i64::MAX } else { end - half },
        });
        if last {
            return windows;
        }
        start = end - WINDOW_OVERLAP_MS;
    }
}

/// Text with its place in the recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedText {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Adds what the speech model returned for `window` (segment times relative to the window) to
/// `transcript` on the meeting's timeline, keeping only the segments this window owns and
/// dropping a repeat of the previous segment across the cut. A model that returned text without
/// segments contributes one segment spanning the window.
pub fn merge_window(
    transcript: &mut Vec<TimedText>,
    window: &AudioWindow,
    text: &str,
    segments: Vec<TimedText>,
) {
    let mut placed = Vec::with_capacity(segments.len().max(1));
    if segments.is_empty() {
        let text = text.trim();
        if !text.is_empty() {
            placed.push(TimedText {
                start_ms: window.keep_from_ms.max(window.start_ms),
                end_ms: window.keep_until_ms.min(window.end_ms),
                text: text.to_owned(),
            });
        }
    }
    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let start = (window.start_ms + segment.start_ms.max(0)).min(window.end_ms);
        let end =
            (window.start_ms + segment.end_ms.max(segment.start_ms)).clamp(start, window.end_ms);
        let middle = start + (end - start) / 2;
        if middle >= window.keep_from_ms && middle < window.keep_until_ms {
            placed.push(TimedText {
                start_ms: start,
                end_ms: end,
                text: text.to_owned(),
            });
        }
    }
    for segment in placed {
        let repeated = transcript.last().is_some_and(|last| {
            segment.start_ms < last.end_ms && same_words(&last.text, &segment.text)
        });
        if !repeated {
            transcript.push(segment);
        }
    }
}

fn same_words(a: &str, b: &str) -> bool {
    tokens(a).eq(tokens(b))
}
