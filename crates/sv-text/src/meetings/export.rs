// FilePath: crates/sv-text/src/meetings/export.rs
//! Rendering a meeting as Markdown or plain text for export.

use super::*;

/// A meeting as Markdown: title, when, summary with action items, then the transcript.
/// `started` is the start time already formatted for the reader.
pub fn export_markdown(detail: &MeetingDetail, started: &str) -> String {
    let meeting = &detail.meeting;
    let mut out = format!(
        "# {}\n\n{started} · {}\n",
        one_line(&meeting.title),
        format_timestamp(meeting.duration_ms)
    );
    if let Some(summary) = &detail.summary {
        let _ = write!(out, "\n## Summary\n\n{}\n", summary.text.trim());
        if !summary.action_items.is_empty() {
            out.push_str("\n### Action items\n\n");
            for item in &summary.action_items {
                let _ = write!(out, "- [ ] {}", one_line(&item.text));
                if let Some(owner) = &item.owner {
                    let _ = write!(out, " — {owner}");
                }
                if let Some(start) = item.start_ms {
                    let _ = write!(out, " ({})", format_timestamp(start));
                }
                out.push('\n');
            }
        }
    }
    out.push_str("\n## Transcript\n");
    for line in transcript_lines(detail) {
        let _ = write!(
            out,
            "\n**[{}] {}:** {}\n",
            format_timestamp(line.start_ms),
            line.speaker,
            one_line(line.text)
        );
    }
    out
}

/// A meeting as plain text with the same content as [`export_markdown`].
pub fn export_text(detail: &MeetingDetail, started: &str) -> String {
    let meeting = &detail.meeting;
    let mut out = format!(
        "{}\n{started}, {}\n",
        one_line(&meeting.title),
        format_timestamp(meeting.duration_ms)
    );
    if let Some(summary) = &detail.summary {
        let _ = write!(out, "\nSUMMARY\n{}\n", summary.text.trim());
        if !summary.action_items.is_empty() {
            out.push_str("\nACTION ITEMS\n");
            for item in &summary.action_items {
                let _ = write!(out, "- {}", one_line(&item.text));
                if let Some(owner) = &item.owner {
                    let _ = write!(out, " ({owner})");
                }
                if let Some(start) = item.start_ms {
                    let _ = write!(out, " [{}]", format_timestamp(start));
                }
                out.push('\n');
            }
        }
    }
    out.push_str("\nTRANSCRIPT\n");
    for line in transcript_lines(detail) {
        let _ = writeln!(
            out,
            "[{}] {}: {}",
            format_timestamp(line.start_ms),
            line.speaker,
            one_line(line.text)
        );
    }
    out
}
