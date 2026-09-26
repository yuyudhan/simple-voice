// FilePath: crates/sv-text/src/preview.rs
//! The one-line preview the dictation pill shows after a paste, so the user can see what was
//! heard without the pill growing across the screen.

/// Longest preview before it is cut, in characters (not bytes: Devanagari counts per letter).
pub const PREVIEW_MAX_CHARS: usize = 80;
const ELLIPSIS: &str = "...";

/// The start of `text` on one line: list markers dropped, whitespace collapsed, and anything
/// past [`PREVIEW_MAX_CHARS`] cut after the last whole word that fits, followed by `...`.
pub fn preview(text: &str) -> String {
    let line = text
        .lines()
        .map(strip_list_marker)
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ");
    let Some((cut, _)) = line.char_indices().nth(PREVIEW_MAX_CHARS) else {
        return line;
    };
    let head = &line[..cut];
    // A word cut in half reads as a typo, so fall back to the previous word boundary; a single
    // word longer than the limit has none and is cut as it is.
    let whole_words = if line[cut..].starts_with(' ') {
        head
    } else {
        head.rsplit_once(' ').map_or(head, |(start, _)| start)
    };
    // "send it to Sam, ..." reads worse than "send it to Sam...".
    let trimmed = whole_words.trim_end_matches(|c: char| {
        c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.' | '-' | '–' | '—')
    });
    let head = if trimmed.is_empty() { head } else { trimmed };
    format!("{head}{ELLIPSIS}")
}

/// Drops the `- `, `* `, `• `, `1. ` or `1) ` the formatter puts in front of list items.
fn strip_list_marker(line: &str) -> &str {
    let line = line.trim_start();
    for bullet in ["- ", "* ", "• "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            return rest;
        }
    }
    let number = line.trim_start_matches(|c: char| c.is_ascii_digit());
    if number.len() < line.len() {
        if let Some(rest) = number
            .strip_prefix(". ")
            .or_else(|| number.strip_prefix(") "))
        {
            return rest;
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(preview("Send it to Sam."), "Send it to Sam.");
    }

    #[test]
    fn text_at_the_limit_is_not_cut() {
        let text = "a".repeat(PREVIEW_MAX_CHARS);
        assert_eq!(preview(&text), text);
    }

    #[test]
    fn long_text_is_cut_after_the_last_whole_word() {
        let text = "Can you send the quarterly report to the whole team before Friday evening \
                    please, and copy the finance leads";
        let shown = preview(text);
        assert_eq!(
            shown,
            "Can you send the quarterly report to the whole team before Friday evening..."
        );
        assert!(shown.chars().count() <= PREVIEW_MAX_CHARS + ELLIPSIS.len());
    }

    #[test]
    fn a_cut_that_lands_on_a_space_keeps_the_last_word() {
        // The character right after the limit is the space before "fifty".
        let text = format!("{} fifty more words follow", "x".repeat(PREVIEW_MAX_CHARS));
        assert_eq!(
            preview(&text),
            format!("{}...", "x".repeat(PREVIEW_MAX_CHARS))
        );
    }

    #[test]
    fn punctuation_before_the_cut_is_dropped() {
        let text = "We should ship the release candidate to everyone on the platform team by \
                    today, tomorrow the notes";
        assert_eq!(
            preview(text),
            "We should ship the release candidate to everyone on the platform team by today..."
        );
    }

    #[test]
    fn one_word_longer_than_the_limit_is_cut_mid_word() {
        let url = format!("https://example.com/{}", "a".repeat(PREVIEW_MAX_CHARS));
        let shown = preview(&url);
        assert_eq!(shown.chars().count(), PREVIEW_MAX_CHARS + ELLIPSIS.len());
        assert!(shown.starts_with("https://example.com/aaa") && shown.ends_with("..."));
    }

    #[test]
    fn lists_become_one_line_without_markers() {
        let text = "Groceries:\n- milk\n- eggs\n\n1. call Sam\n2) book flights";
        assert_eq!(preview(text), "Groceries: milk eggs call Sam book flights");
    }

    #[test]
    fn devanagari_is_counted_in_characters_not_bytes() {
        // Under the limit in characters, far over it in bytes: shown whole.
        let short = "मुझे कल सुबह दस बजे की मीटिंग के बारे में याद दिलाना";
        assert!(short.len() > PREVIEW_MAX_CHARS);
        assert_eq!(preview(short), short);

        let long = format!("{short} और रिपोर्ट भेजना, फिर टीम को ईमेल करना मत भूलना");
        let shown = preview(&long);
        assert!(shown.ends_with("..."));
        assert!(shown.chars().count() <= PREVIEW_MAX_CHARS + ELLIPSIS.len());
        assert!(long.starts_with(shown.trim_end_matches("...")));
    }
}
