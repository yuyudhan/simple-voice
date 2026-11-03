// FilePath: crates/sv-text/src/spoken/chord.rs
//! Spoken key chords: modifiers (control, command, option, alt, shift, fn) joined by "plus",
//! "and" or nothing, ending in a key name, written as the canonical chord: "control plus alt
//! plus delete" → `Ctrl+Alt+Delete`, "command shift p" → `Cmd+Shift+P`, "control backtick" →
//! `` Ctrl+` ``. "all" is heard for "alt" only inside a chord ("control all delete"), and "or"
//! only in the famous "control or delete". A single letter after one modifier other than
//! control or command ("option a") is prose, and so is a modifier not followed by a key
//! ("control over all of it").

use super::{tokens, Edit, Token};

fn modifier(word: &str) -> Option<&'static str> {
    let written = match word {
        "control" | "ctrl" => "Ctrl",
        "command" | "cmd" => "Cmd",
        "option" | "opt" => "Opt",
        "alt" => "Alt",
        "shift" => "Shift",
        "fn" | "function" => "Fn",
        _ => return None,
    };
    Some(written)
}

/// A key name of one or two words (with how many it takes), and whether it is a letter or
/// digit, which needs a stronger chord.
fn key(tokens: &[Token<'_>], at: usize) -> Option<(String, usize, bool)> {
    let word = tokens.get(at)?.lower.as_str();
    let next = tokens
        .get(at + 1)
        .filter(|_| !tokens[at].closed)
        .map(|t| t.lower.as_str());
    let pair = match (word, next) {
        ("back", Some("tick" | "quote")) => Some("`"),
        ("page", Some("up")) => Some("PageUp"),
        ("page", Some("down")) => Some("PageDown"),
        ("forward", Some("slash")) => Some("/"),
        ("left" | "open", Some("bracket")) => Some("["),
        ("right" | "close", Some("bracket")) => Some("]"),
        _ => None,
    };
    if let Some(name) = pair {
        return Some((name.to_owned(), 2, false));
    }
    let named = match word {
        "delete" | "del" => "Delete",
        "backspace" => "Backspace",
        "escape" | "esc" => "Esc",
        "enter" => "Enter",
        "return" => "Return",
        "tab" => "Tab",
        "space" | "spacebar" => "Space",
        "backtick" | "grave" => "`",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "comma" => ",",
        "period" => ".",
        "slash" => "/",
        "backslash" => "\\",
        "minus" | "dash" | "hyphen" => "-",
        "equals" => "=",
        "semicolon" => ";",
        "apostrophe" | "quote" => "'",
        _ => "",
    };
    if !named.is_empty() {
        let arrow = matches!(word, "up" | "down" | "left" | "right") && next == Some("arrow");
        return Some((named.to_owned(), if arrow { 2 } else { 1 }, false));
    }
    let mut chars = word.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        if c.is_ascii_alphanumeric() {
            return Some((c.to_ascii_uppercase().to_string(), 1, true));
        }
    }
    if let Some(n) = word.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=12).contains(&n) {
            return Some((format!("F{n}"), 1, false));
        }
    }
    let digit = match word {
        "zero" => "0",
        "one" => "1",
        "two" => "2",
        "three" => "3",
        "four" => "4",
        "five" => "5",
        "six" => "6",
        "seven" => "7",
        "eight" => "8",
        "nine" => "9",
        _ => return None,
    };
    Some((digit.to_owned(), 1, true))
}

/// The chord starting at `tokens[at]`: its end token index and how it is written.
fn chord_at(tokens: &[Token<'_>], at: usize) -> Option<(usize, String)> {
    let mut modifiers: Vec<&'static str> = vec![modifier(&tokens[at].lower)?];
    let mut index = at + 1;
    loop {
        // "control, then" ends without a key; "Ctrl, Alt, Delete" is too rare to guess at.
        if tokens[index - 1].closed {
            return None;
        }
        let word = tokens.get(index)?.lower.as_str();
        if matches!(word, "plus" | "+" | "and") {
            index += 1;
            continue;
        }
        if let Some(found) = modifier(word) {
            if !modifiers.contains(&found) {
                modifiers.push(found);
            }
            index += 1;
            continue;
        }
        let heard_alt = match word {
            "all" => true,
            "or" => {
                modifiers == ["Ctrl"] && tokens.get(index + 1).is_some_and(|t| t.lower == "delete")
            }
            _ => false,
        };
        if heard_alt && !modifiers.contains(&"Alt") {
            key(tokens, index + 1)?;
            modifiers.push("Alt");
            index += 1;
            continue;
        }
        let (name, used, letter) = key(tokens, index)?;
        let strong = modifiers.len() >= 2 || matches!(modifiers[0], "Ctrl" | "Cmd");
        let weak_letter = letter && (name == "A" || name == "I") && modifiers.len() < 2;
        if (letter && !strong) || weak_letter {
            return None;
        }
        let mut written = modifiers.join("+");
        written.push('+');
        written.push_str(&name);
        return Some((index + used, written));
    }
}

pub(crate) fn edits(text: &str) -> Vec<Edit> {
    let tokens = split_plus(&tokens(text));
    let mut edits = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        match chord_at(&tokens, at) {
            Some((end, written)) => {
                let (start, stop) = (tokens[at].start, tokens[end - 1].end);
                if text[start..stop] != written {
                    edits.push(Edit {
                        start,
                        end: stop,
                        with: written,
                    });
                }
                at = end;
            }
            None => at += 1,
        }
    }
    edits
}

/// Splits a written chord ("Control+Alt+Delete") into its parts, so it is normalised like a
/// spoken one; every part keeps its own byte range.
fn split_plus<'a>(tokens: &[Token<'a>]) -> Vec<Token<'a>> {
    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        if !token.core.contains('+') || token.core.len() < 3 {
            out.push(token.clone());
            continue;
        }
        let mut offset = token.start;
        let parts: Vec<&str> = token.core.split('+').collect();
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                offset += 1;
                continue;
            }
            let last = i + 1 == parts.len();
            out.push(Token {
                start: offset,
                end: offset + part.len(),
                core: part,
                lower: part.to_lowercase(),
                closed: last && token.closed,
            });
            offset += part.len() + 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chords(text: &str) -> String {
        super::super::splice(text, edits(text))
    }

    #[test]
    fn spoken_chords_become_canonical() {
        for (said, written) in [
            (
                "Press control plus alt plus delete.",
                "Press Ctrl+Alt+Delete.",
            ),
            ("Press control alt delete", "Press Ctrl+Alt+Delete"),
            ("Control all delete.", "Ctrl+Alt+Delete."),
            ("Press Ctrl or Delete.", "Press Ctrl+Alt+Delete."),
            (
                "Hit command shift p and type reload",
                "Hit Cmd+Shift+P and type reload",
            ),
            (
                "Use option command escape to force quit",
                "Use Opt+Cmd+Esc to force quit",
            ),
            ("Toggle it with control backtick.", "Toggle it with Ctrl+`."),
            (
                "Toggle it with control back tick.",
                "Toggle it with Ctrl+`.",
            ),
            ("press command and k", "press Cmd+K"),
            ("shift tab goes back", "Shift+Tab goes back"),
            ("Control+alt+delete", "Ctrl+Alt+Delete"),
            ("try command shift a", "try Cmd+Shift+A"),
            ("press fn f5", "press Fn+F5"),
        ] {
            assert_eq!(chords(said), written, "{said}");
        }
    }

    #[test]
    fn prose_with_modifier_words_stays() {
        for text in [
            "I have control over all of it.",
            "Option a is better than option b.",
            "Shift the meeting to Monday.",
            "We lost control, then it crashed.",
            "The command line and the alt text.",
            "Take control a bit more.",
            "control all of it",
            "Ctrl+Alt+Delete",
        ] {
            assert_eq!(chords(text), text, "{text}");
        }
    }
}
