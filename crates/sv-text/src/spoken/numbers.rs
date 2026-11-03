// FilePath: crates/sv-text/src/spoken/numbers.rs
//! Numbers as speech models write them, in words ("twenty five", "five hundred") or digits, and
//! the technical units that follow them: "five hundred milliseconds" → `500 ms`. Units of
//! everyday prose ("five minutes", "ten percent") are left to the formatting model.

use super::{tokens, Edit, Token};

fn small(word: &str) -> Option<u64> {
    let value = match word {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    };
    Some(value)
}

/// The number starting at `tokens[at]` and how many tokens it spans: digits ("12", "2.5") or
/// number words ("twenty five", "twenty-five", "five hundred", "two thousand and ten").
pub(crate) fn number(tokens: &[Token<'_>], at: usize) -> Option<(String, usize)> {
    let first = tokens.get(at)?;
    if is_digits(first.core) {
        return Some((first.core.to_owned(), 1));
    }
    let mut total: u64 = 0;
    let mut current: u64 = 0;
    let mut used = 0;
    let mut seen = false;
    while let Some(token) = tokens.get(at + used) {
        let mut parts = token.lower.split('-');
        let word = parts.next().unwrap_or_default();
        let extra = parts.next();
        if let Some(value) = small(word) {
            if seen && current % 10 != 0 && current % 100 != 0 {
                break;
            }
            current += value;
            if let Some(unit) = extra.and_then(small) {
                current += unit;
            }
        } else if word == "hundred" && seen {
            current = current.max(1) * 100;
        } else if word == "thousand" && seen {
            total += current.max(1) * 1_000;
            current = 0;
        } else if word == "and"
            && seen
            && tokens
                .get(at + used + 1)
                .is_some_and(|t| small(t.lower.split('-').next().unwrap_or_default()).is_some())
            && total + current >= 100
        {
            used += 1;
            continue;
        } else {
            break;
        }
        seen = true;
        used += 1;
        if token.closed {
            break;
        }
    }
    seen.then(|| ((total + current).to_string(), used))
}

fn is_digits(word: &str) -> bool {
    let mut dots = 0;
    let mut digits = 0;
    for c in word.chars() {
        match c {
            '0'..='9' => digits += 1,
            '.' => dots += 1,
            _ => return false,
        }
    }
    digits > 0 && dots <= 1 && !word.starts_with('.') && !word.ends_with('.')
}

fn unit(word: &str) -> Option<&'static str> {
    let unit = match word {
        "millisecond" | "milliseconds" | "ms" => "ms",
        "kilobyte" | "kilobytes" => "KB",
        "megabyte" | "megabytes" => "MB",
        "gigabyte" | "gigabytes" => "GB",
        "terabyte" | "terabytes" => "TB",
        _ => return None,
    };
    Some(unit)
}

/// "five hundred milliseconds" → `500 ms`, "two gigabytes" → `2 GB`.
pub(crate) fn unit_edits(text: &str) -> Vec<Edit> {
    let tokens = tokens(text);
    let mut edits = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let Some((value, used)) = number(&tokens, at) else {
            at += 1;
            continue;
        };
        let last = at + used - 1;
        let written = tokens.get(at + used).filter(|_| !tokens[last].closed);
        match written.and_then(|token| unit(&token.lower)) {
            Some(unit) => {
                let with = format!("{value} {unit}");
                let end = tokens[at + used].end;
                if text[tokens[at].start..end] != with {
                    edits.push(Edit {
                        start: tokens[at].start,
                        end,
                        with,
                    });
                }
                at += used + 1;
            }
            None => at += used,
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Option<(String, usize)> {
        number(&tokens(text), 0)
    }

    fn units(text: &str) -> String {
        super::super::splice(text, unit_edits(text))
    }

    #[test]
    fn number_words_and_digits() {
        assert_eq!(parse("six"), Some(("6".to_owned(), 1)));
        assert_eq!(parse("twenty five apples"), Some(("25".to_owned(), 2)));
        assert_eq!(parse("twenty-five"), Some(("25".to_owned(), 1)));
        assert_eq!(parse("five hundred ms"), Some(("500".to_owned(), 2)));
        assert_eq!(parse("two thousand and ten"), Some(("2010".to_owned(), 4)));
        assert_eq!(parse("12"), Some(("12".to_owned(), 1)));
        assert_eq!(parse("2.5"), Some(("2.5".to_owned(), 1)));
        // "two two" is two numbers, not twenty-two or four.
        assert_eq!(parse("two two"), Some(("2".to_owned(), 1)));
        assert_eq!(parse("hundred"), None);
        assert_eq!(parse("v2"), None);
        assert_eq!(parse("and two"), None);
    }

    #[test]
    fn technical_units_become_symbols() {
        assert_eq!(
            units("Set the timeout to five hundred milliseconds."),
            "Set the timeout to 500 ms."
        );
        assert_eq!(units("It uses 512 megabytes"), "It uses 512 MB");
        assert_eq!(units("about two gigabytes, I think"), "about 2 GB, I think");
    }

    #[test]
    fn everyday_units_and_lone_numbers_stay() {
        for text in [
            "Give me five minutes.",
            "Ten percent of the team.",
            "One of them, milliseconds later.",
            "We saw 500 ms already.",
        ] {
            assert_eq!(units(text), text, "{text}");
        }
    }
}
