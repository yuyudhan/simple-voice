// FilePath: crates/sv-text/src/meetings/mod.rs
//! The pure half of personal meetings: cutting a long recording into windows a speech model can
//! transcribe and stitching the pieces back together, giving each transcript segment the
//! diarized speaker it overlaps most, and the prompts and strict reply parsers for the
//! on-device language model's summary, transcript-grounded answers and translations. Long
//! meetings never reach a model whole: every prompt is built from bounded chunks.

use std::fmt::Write as _;
use std::ops::Range;

use sv_domain::{ActionItem, Citation, MeetingDetail};

use crate::polish::{PolishPrompt, PolishTarget};

mod chat;
mod export;
mod speakers;
mod summary;
mod windows;

pub use self::chat::{
    chat_excerpt_chars, chat_prompt, parse_chat_answer, retrieve_excerpts, ChatAnswer, Excerpts,
};
pub use self::export::{export_markdown, export_text};
pub use self::speakers::{
    assign_speakers, format_timestamp, parse_timestamp, transcript_lines, AttributedSegment,
    Attribution, SpeakerSpan, TranscriptLine,
};
pub use self::summary::{
    join_bounded, parse_summary, render_summary, summary_chunk_chars, summary_chunks,
    summary_merge_prompt, summary_prompt, ParsedSummary,
};
pub use self::windows::{audio_windows, merge_window, AudioWindow, TimedText};

/// Audio per transcription request. The helper allows one request two minutes; four minutes
/// of audio transcribes well inside that on Apple Silicon with every offered model.
pub const WINDOW_MS: i64 = 240_000;
/// Consecutive windows share this much audio so no word is lost at a cut.
pub const WINDOW_OVERLAP_MS: i64 = 4_000;

pub const SUMMARY_MAX_TOKENS: u32 = 900;
pub const CHAT_MAX_TOKENS: u32 = 600;
/// Excerpt groups an answer is grounded in at most.
const MAX_EXCERPT_GROUPS: usize = 12;
/// Neighbouring segments shown on each side of a matching one.
const CONTEXT_RADIUS: usize = 1;
/// Words of a question used for retrieval at most.
const MAX_QUERY_TERMS: usize = 32;
/// One transcript segment never takes more than this much of a prompt.
const MAX_LINE_CHARS: usize = 800;
const UNKNOWN_SPEAKER: &str = "Unknown speaker";

const STOPWORDS: &[&str] = &[
    "a", "about", "after", "all", "also", "an", "and", "any", "are", "as", "at", "be", "been",
    "but", "by", "can", "could", "did", "do", "does", "for", "from", "had", "has", "have", "he",
    "her", "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "me", "meeting", "my",
    "no", "not", "of", "on", "or", "our", "said", "say", "she", "so", "that", "the", "their",
    "them", "then", "there", "they", "this", "to", "us", "was", "we", "were", "what", "when",
    "where", "which", "who", "why", "will", "with", "would", "you", "your",
];
/// Lowercased runs of letters and digits.
fn tokens(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}
/// Keeps at most `max` characters, cutting on a character boundary.
fn clip(text: &str, max: usize) -> &str {
    text.char_indices()
        .nth(max)
        .map_or(text, |(at, _)| &text[..at])
}

/// One line of text: inner line breaks become spaces.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
/// Splits segment texts into consecutive batches of at most `max_chars` (a longer segment is a
/// batch of its own).
pub fn translation_batches(texts: &[&str], max_chars: usize) -> Vec<Range<usize>> {
    let mut batches = Vec::new();
    let mut start = 0;
    let mut chars = 0;
    for (index, text) in texts.iter().enumerate() {
        let length = text.chars().count();
        if index > start && chars + length > max_chars {
            batches.push(start..index);
            start = index;
            chars = 0;
        }
        chars += length;
    }
    if start < texts.len() {
        batches.push(start..texts.len());
    }
    batches
}

#[cfg(test)]
mod tests {
    use sv_domain::{Meeting, MeetingStatus, MeetingSummary, Segment, Speaker};

    use super::*;

    fn timed(start_ms: i64, end_ms: i64, text: &str) -> TimedText {
        TimedText {
            start_ms,
            end_ms,
            text: text.to_owned(),
        }
    }

    fn turn(start_ms: i64, end_ms: i64, speaker: &str) -> SpeakerSpan {
        SpeakerSpan {
            start_ms,
            end_ms,
            speaker: speaker.to_owned(),
        }
    }

    fn line(
        id: i64,
        start_ms: i64,
        speaker: &'static str,
        text: &'static str,
    ) -> TranscriptLine<'static> {
        TranscriptLine {
            id,
            start_ms,
            speaker,
            text,
        }
    }

    #[test]
    fn windows_cover_the_recording_with_overlap_and_disjoint_ownership() {
        assert!(audio_windows(0).is_empty());
        let short = audio_windows(90_000);
        assert_eq!(
            short,
            vec![AudioWindow {
                start_ms: 0,
                end_ms: 90_000,
                keep_from_ms: 0,
                keep_until_ms: i64::MAX
            }]
        );

        let windows = audio_windows(600_000);
        assert_eq!(windows.first().map(|w| w.start_ms), Some(0));
        assert_eq!(windows.last().map(|w| w.end_ms), Some(600_000));
        for pair in windows.windows(2) {
            assert_eq!(pair[1].start_ms, pair[0].end_ms - WINDOW_OVERLAP_MS);
            // Ownership hands over exactly where the previous window stops keeping.
            assert_eq!(pair[1].keep_from_ms, pair[0].keep_until_ms);
        }
        assert!(windows.iter().all(|w| w.end_ms - w.start_ms <= WINDOW_MS));
    }

    #[test]
    fn merging_windows_offsets_times_and_drops_the_overlap_repeat() {
        let windows = audio_windows(WINDOW_MS + 60_000);
        let mut transcript = Vec::new();
        merge_window(
            &mut transcript,
            &windows[0],
            "",
            vec![
                timed(1_000, 4_000, "hello everyone"),
                // Its midpoint is before the hand-over point: this window owns it.
                timed(WINDOW_MS - 3_500, WINDOW_MS - 1_000, "the budget"),
                // Its midpoint is after the hand-over point: the next window owns it.
                timed(WINDOW_MS - 900, WINDOW_MS, "for march"),
            ],
        );
        // The second window starts 4 s before the first ends and hears those words again.
        merge_window(
            &mut transcript,
            &windows[1],
            "",
            vec![
                // The same words, cut a little later: a repeat across the cut.
                timed(1_000, 3_500, "The budget."),
                timed(3_100, 4_000, "for March"),
                timed(5_000, 9_000, "is approved"),
                timed(9_000, 9_000, "  "),
            ],
        );
        let texts: Vec<(&str, i64)> = transcript
            .iter()
            .map(|segment| (segment.text.as_str(), segment.start_ms))
            .collect();
        let second_start = windows[1].start_ms;
        assert_eq!(second_start, WINDOW_MS - WINDOW_OVERLAP_MS);
        assert_eq!(
            texts,
            vec![
                ("hello everyone", 1_000),
                ("the budget", WINDOW_MS - 3_500),
                ("for March", second_start + 3_100),
                ("is approved", second_start + 5_000),
            ]
        );
    }

    #[test]
    fn a_window_without_segments_contributes_its_text_once() {
        let window = audio_windows(30_000)[0];
        let mut transcript = Vec::new();
        merge_window(&mut transcript, &window, "  just text ", Vec::new());
        merge_window(&mut transcript, &window, "   ", Vec::new());
        assert_eq!(transcript, vec![timed(0, 30_000, "just text")]);
    }

    #[test]
    fn speakers_follow_the_longest_overlap_and_are_numbered_by_first_appearance() {
        let segments = vec![
            timed(5_000, 9_000, "second"),
            timed(0, 4_000, "first"),
            timed(20_000, 22_000, "nobody"),
            timed(10_000, 16_000, "mostly S1"),
        ];
        let turns = vec![
            turn(0, 4_500, "S2"),
            turn(4_500, 9_500, "S1"),
            turn(10_000, 12_000, "S2"),
            turn(12_000, 16_000, "S1"),
            turn(30_000, 31_000, "S3"),
        ];
        let attribution = assign_speakers(segments, &turns);
        let got: Vec<(&str, Option<usize>)> = attribution
            .segments
            .iter()
            .map(|segment| (segment.text.as_str(), segment.speaker))
            .collect();
        assert_eq!(
            got,
            vec![
                ("first", Some(0)),
                ("second", Some(1)),
                ("mostly S1", Some(1)),
                ("nobody", None),
            ]
        );
        // S3 owns no segment, so it is not a speaker of the transcript.
        assert_eq!(attribution.speaker_count, 2);
    }

    #[test]
    fn split_turns_of_one_speaker_add_up() {
        let turns = vec![
            turn(0, 1_000, "S1"),
            turn(1_000, 2_500, "S2"),
            turn(2_500, 4_000, "S1"),
        ];
        let attribution = assign_speakers(vec![timed(0, 4_000, "x")], &turns);
        assert_eq!(attribution.segments[0].speaker, Some(0));
        assert_eq!(attribution.speaker_count, 1);
    }

    #[test]
    fn timestamps_format_and_parse_back() {
        assert_eq!(format_timestamp(0), "00:00");
        assert_eq!(format_timestamp(-5), "00:00");
        assert_eq!(format_timestamp(65_999), "01:05");
        assert_eq!(format_timestamp(3_723_000), "1:02:03");
        assert_eq!(parse_timestamp("01:05"), Some(65_000));
        assert_eq!(parse_timestamp("1:02:03"), Some(3_723_000));
        assert_eq!(parse_timestamp("1:75"), None);
        assert_eq!(parse_timestamp("abc"), None);
        assert_eq!(parse_timestamp("5"), None);
    }

    #[test]
    fn summary_chunks_stay_within_budget_and_keep_order() {
        let lines = vec![
            line(1, 0, "Ana", "one two three"),
            line(2, 61_000, "Ben", "four five six"),
            line(3, 3_600_000, "Ana", "seven"),
        ];
        let chunks = summary_chunks(&lines, 60);
        assert_eq!(
            chunks,
            vec![
                "[00:00] Ana: one two three\n[01:01] Ben: four five six".to_owned(),
                "[1:00:00] Ana: seven".to_owned(),
            ]
        );
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 60));
    }

    #[test]
    fn parses_a_summary_with_action_items() {
        let reply = "Here is the summary:\n**SUMMARY:**\nThe beta ships Friday.\n\nLaunch date \
                     open.\n## ACTION ITEMS:\n- 00:11 | Priya | Fix the login bug\n* - | - | \
                     Pick a launch date\n";
        let summary = parse_summary(reply).unwrap();
        assert_eq!(summary.text, "The beta ships Friday.\n\nLaunch date open.");
        assert_eq!(
            summary.action_items,
            vec![
                ActionItem {
                    text: "Fix the login bug".to_owned(),
                    owner: Some("Priya".to_owned()),
                    start_ms: Some(11_000),
                },
                ActionItem {
                    text: "Pick a launch date".to_owned(),
                    owner: None,
                    start_ms: None,
                },
            ]
        );
        // The rendered form parses back to the same summary (merge input).
        assert_eq!(parse_summary(&render_summary(&summary)).unwrap(), summary);
    }

    #[test]
    fn summary_without_action_items_says_none() {
        let summary = parse_summary("SUMMARY: Short sync.\nACTION ITEMS:\nNONE").unwrap();
        assert_eq!(summary.text, "Short sync.");
        assert!(summary.action_items.is_empty());
    }

    #[test]
    fn rejects_replies_that_are_not_a_summary() {
        assert!(parse_summary("Sure! The meeting was about the beta.").is_err());
        assert!(parse_summary("SUMMARY:\nCut off before the act").is_err());
        assert!(parse_summary("SUMMARY:\n\nACTION ITEMS:\nNONE").is_err());
        assert!(parse_summary("SUMMARY:\nok\nACTION ITEMS:\nFix it").is_err());
        assert!(parse_summary("SUMMARY:\nok\nACTION ITEMS:\n- 99:99 | A | B").is_err());
        assert!(parse_summary("SUMMARY:\nok\nACTION ITEMS:\n- 00:01 | A |  ").is_err());
    }

    #[test]
    fn retrieval_prefers_rare_matching_words_with_context_and_bounds_size() {
        let lines = vec![
            line(10, 0, "Ana", "Welcome everyone to the weekly sync"),
            line(11, 5_000, "Ben", "The database migration failed last night"),
            line(12, 9_000, "Ana", "Who owns the rollback"),
            line(13, 15_000, "Ben", "Lunch is on Friday"),
            line(14, 20_000, "Ana", "Weekly numbers look fine"),
        ];
        let excerpts = retrieve_excerpts(&lines, "Why did the migration fail?", 10_000);
        let ids: Vec<i64> = excerpts.segments.iter().map(|c| c.segment_id).collect();
        assert_eq!(ids, vec![10, 11, 12]);
        assert!(excerpts
            .text
            .starts_with("[#10 00:00 Ana] Welcome everyone to the weekly sync\n[#11 00:05 Ben]"));

        let tight = retrieve_excerpts(&lines, "migration", 60);
        let ids: Vec<i64> = tight.segments.iter().map(|c| c.segment_id).collect();
        assert_eq!(ids, vec![11]);
        assert!(tight.text.chars().count() <= 60);
    }

    #[test]
    fn retrieval_without_matching_words_spreads_over_the_meeting() {
        let lines: Vec<TranscriptLine<'static>> = (0..40)
            .map(|i| line(i, i * 1_000, "Ana", "words"))
            .collect();
        let excerpts = retrieve_excerpts(&lines, "what happened?", 100_000);
        let ids: Vec<i64> = excerpts.segments.iter().map(|c| c.segment_id).collect();
        assert!(ids.contains(&1) && ids.contains(&38), "{ids:?}");
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn chat_answers_keep_only_citations_of_given_excerpts() {
        let excerpts = Excerpts {
            text: String::new(),
            segments: vec![
                Citation {
                    segment_id: 11,
                    start_ms: 5_000,
                },
                Citation {
                    segment_id: 12,
                    start_ms: 9_000,
                },
            ],
        };
        let answer = parse_chat_answer(
            "The migration failed [#11, #99]. Ben owns it [#12] [#77]. See [notes].",
            &excerpts,
        )
        .unwrap();
        assert_eq!(
            answer.text,
            "The migration failed [1]. Ben owns it [2]. See [notes]."
        );
        assert_eq!(answer.citations, excerpts.segments);
        assert!(parse_chat_answer("  [#99] ", &excerpts).is_err());

        let none = parse_chat_answer("That is not in the transcript.", &excerpts).unwrap();
        assert!(none.citations.is_empty());

        // A segment cited again keeps its first number; no internal `#id` reaches the text.
        let again =
            parse_chat_answer("Ben [#12] fixed it, see [#11, #12, #12].", &excerpts).unwrap();
        assert_eq!(again.text, "Ben [1] fixed it, see [2, 1].");
        assert!(!again.text.contains('#'));
        assert_eq!(again.citations[0].segment_id, 12);
    }

    #[test]
    fn chat_prompt_carries_excerpts_and_question() {
        let lines = vec![line(3, 61_000, "Ben", "Ship it Friday")];
        let excerpts = retrieve_excerpts(&lines, "When do we ship?", 1_000);
        let prompt = chat_prompt(&excerpts, "When do we\nship?");
        assert!(prompt.user.contains("[#3 01:01 Ben] Ship it Friday"));
        assert!(prompt.user.ends_with("Question: When do we ship?"));
    }

    #[test]
    fn translation_batches_are_bounded_and_contiguous() {
        let texts = ["aaaa", "bbbb", "cc", "dddddddddddd", "e"];
        assert_eq!(translation_batches(&texts, 10), vec![0..3, 3..4, 4..5]);
        assert!(translation_batches(&[], 10).is_empty());
    }

    fn detail() -> MeetingDetail {
        MeetingDetail {
            meeting: Meeting {
                id: 1,
                title: "Weekly sync".to_owned(),
                started_at: 0,
                ended_at: Some(65_000),
                duration_ms: 65_000,
                status: MeetingStatus::Ready,
                microphone: true,
                system_audio: false,
                language: Some("en".to_owned()),
                error: None,
                segment_count: 3,
                speaker_count: 2,
            },
            speakers: vec![
                Speaker {
                    id: 7,
                    label: "Speaker 1".to_owned(),
                    name: Some("Ana".to_owned()),
                },
                Speaker {
                    id: 8,
                    label: "Speaker 2".to_owned(),
                    name: None,
                },
            ],
            segments: vec![
                Segment {
                    id: 1,
                    start_ms: 0,
                    end_ms: 3_000,
                    speaker_id: Some(7),
                    text: "Hello".to_owned(),
                },
                Segment {
                    id: 2,
                    start_ms: 61_000,
                    end_ms: 64_000,
                    speaker_id: Some(8),
                    text: "Hi Ana".to_owned(),
                },
                Segment {
                    id: 3,
                    start_ms: 64_000,
                    end_ms: 65_000,
                    speaker_id: None,
                    text: "(laughs)".to_owned(),
                },
            ],
            summary: Some(MeetingSummary {
                text: "A greeting.".to_owned(),
                action_items: vec![ActionItem {
                    text: "Send notes".to_owned(),
                    owner: Some("Ana".to_owned()),
                    start_ms: Some(61_000),
                }],
                model: "qwen3-4b-instruct".to_owned(),
                created_at: 0,
            }),
            chat: Vec::new(),
            translations: Vec::new(),
        }
    }

    #[test]
    fn exports_render_speaker_names_and_timestamps() {
        let detail = detail();
        let markdown = export_markdown(&detail, "2026-10-13 09:00");
        assert_eq!(
            markdown,
            "# Weekly sync\n\n2026-10-13 09:00 · 01:05\n\n## Summary\n\nA greeting.\n\n\
             ### Action items\n\n- [ ] Send notes — Ana (01:01)\n\n## Transcript\n\n\
             **[00:00] Ana:** Hello\n\n**[01:01] Speaker 2:** Hi Ana\n\n\
             **[01:04] Unknown speaker:** (laughs)\n"
        );
        let text = export_text(&detail, "2026-10-13 09:00");
        assert!(text.contains("\nTRANSCRIPT\n[00:00] Ana: Hello\n[01:01] Speaker 2: Hi Ana\n"));
        assert!(text.contains("- Send notes (Ana) [01:01]"));
    }
}
