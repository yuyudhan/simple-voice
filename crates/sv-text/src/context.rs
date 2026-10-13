// FilePath: crates/sv-text/src/context.rs
//! Spelling hints read from the app the user is dictating into: the text of the focused field
//! around the caret, as the engine's `focus_context` returns it, and in an IDE the window title.
//! Only words whose spelling a model could get wrong are kept (names, acronyms, brand casing; in
//! an IDE also file names and code identifiers), bounded in count and length. Emails, URLs,
//! paths, numbers and secret-looking tokens are never kept, and nothing here reads files or
//! stores the text. `Vocabulary::add_context_terms` drops terms the vocabulary already has.

use std::collections::{HashMap, HashSet};

/// The most text, in UTF-16 units, to ask the engine's `focus_context` for.
pub const FOCUS_MAX_CHARS: u32 = 2_000;
/// The most terms one dictation takes from its context.
pub const CONTEXT_MAX_TERMS: usize = 24;
/// The most characters all of a dictation's context terms may add up to.
pub const CONTEXT_MAX_CHARS: usize = 400;
/// The longest single term, in characters; anything longer is treated as data, not a word.
pub const CONTEXT_TERM_MAX_CHARS: usize = 40;

const HINT_MIN_CHARS: usize = 2;
const IDENTIFIER_MIN_CHARS: usize = 3;
const ACRONYM_MAX_CHARS: usize = 8;
/// A token with this many digits reads as a number, id or key rather than a word.
const SECRET_MIN_DIGITS: usize = 3;
/// Longer tokens with any digit read as ids or access keys rather than words.
const DIGIT_WORD_MAX_CHARS: usize = 8;
/// From this length, a token flipping case on more than a third of its letters is random.
const RANDOM_CASE_MIN_CHARS: usize = 12;
/// Capitals everyone spells right.
const COMMON_ACRONYMS: [&str; 4] = ["OK", "AM", "PM", "ID"];
/// Extensions recognised as file names, so a member chain like `user.name.length` is not one.
const CODE_EXTENSIONS: [&str; 47] = [
    "c", "cc", "cfg", "cpp", "cs", "css", "dart", "env", "ex", "exs", "go", "gradle", "h", "hpp",
    "html", "ini", "java", "js", "json", "jsx", "kt", "lock", "lua", "m", "md", "mjs", "mm", "php",
    "plist", "py", "rb", "rs", "sass", "scala", "scss", "sh", "sql", "svelte", "swift", "toml",
    "ts", "tsx", "txt", "vue", "xml", "yaml", "yml",
];

/// Names, acronyms and mixed-case words from the focused field, nearest the caret first.
/// `caret_utf16` is a UTF-16 offset into `text`; `None` or past the end means the end.
pub fn context_terms(text: &str, caret_utf16: Option<u32>) -> Vec<String> {
    let (window, caret) = bounded(text, caret_utf16);
    let mut found: Vec<(usize, &str)> = words(&window)
        .into_iter()
        .filter_map(|word| {
            let hint = hint_word(word.text).filter(|hint| is_hint(hint, word.sentence_start))?;
            let distance = if word.end <= caret {
                caret - word.end
            } else {
                word.start.saturating_sub(caret)
            };
            Some((distance, hint))
        })
        .collect();
    found.sort_by_key(|(distance, _)| *distance);
    let mut terms = Terms::default();
    for (_, hint) in found {
        terms.push(hint);
    }
    terms.terms
}

/// File names (from the window title, as editors show the open file there, and from the text)
/// and camelCase, PascalCase, snake_case or SCREAMING_CASE identifiers visible in an IDE's
/// focused text. Title file names come first, then the text's terms, most frequent first. Paths
/// keep only their file name.
pub fn ide_terms(text: &str, window_title: Option<&str>) -> Vec<String> {
    let mut terms = Terms::default();
    for chunk in window_title.unwrap_or_default().split_whitespace() {
        if let Some(name) = file_name(chunk) {
            terms.push(name);
        }
    }

    let (window, _) = bounded(text, None);
    let mut tallies: HashMap<String, Tally<'_>> = HashMap::new();
    let chunks = window
        .split(|c: char| !is_code_char(c))
        .filter(|chunk| !chunk.is_empty());
    for (order, term) in chunks.flat_map(code_terms).enumerate() {
        let tally = tallies.entry(term.to_lowercase()).or_insert(Tally {
            term,
            count: 0,
            first: order,
        });
        tally.count += 1;
    }
    let mut ranked: Vec<Tally<'_>> = tallies.into_values().collect();
    ranked.sort_by_key(|tally| (std::cmp::Reverse(tally.count), tally.first));
    for tally in ranked {
        terms.push(tally.term);
    }
    terms.terms
}

struct Tally<'a> {
    term: &'a str,
    count: usize,
    first: usize,
}

/// The chosen terms, each once whatever its case, within the count and character caps.
#[derive(Default)]
struct Terms {
    terms: Vec<String>,
    seen: HashSet<String>,
    chars: usize,
}

impl Terms {
    fn push(&mut self, term: &str) {
        let chars = term.chars().count();
        if self.terms.len() >= CONTEXT_MAX_TERMS || self.chars + chars > CONTEXT_MAX_CHARS {
            return;
        }
        if self.seen.insert(term.to_lowercase()) {
            self.chars += chars;
            self.terms.push(term.to_owned());
        }
    }
}

/// At most [`FOCUS_MAX_CHARS`] characters of `text` around the caret (two thirds before it),
/// without the words the cut goes through, and the caret's character index in that window.
fn bounded(text: &str, caret: Option<u32>) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret_index(&chars, caret);
    let max = usize::try_from(FOCUS_MAX_CHARS).unwrap_or(usize::MAX);
    if chars.len() <= max {
        return (text.to_owned(), caret);
    }
    let mut start = caret.saturating_sub(max * 2 / 3).min(chars.len() - max);
    let mut end = start + max;
    let cut_at_start = chars
        .get(start.saturating_sub(1))
        .is_some_and(|c| !c.is_whitespace());
    if start > 0 && cut_at_start {
        let inside = chars.get(start..end).unwrap_or_default();
        start += inside
            .iter()
            .position(|c| c.is_whitespace())
            .unwrap_or(inside.len());
    }
    if chars.get(end).is_some_and(|c| !c.is_whitespace()) {
        let inside = chars.get(start..end).unwrap_or_default();
        end = start + inside.iter().rposition(|c| c.is_whitespace()).unwrap_or(0);
    }
    let window = chars.get(start..end).unwrap_or_default().iter().collect();
    (window, caret.saturating_sub(start).min(end - start))
}

/// Character index of a UTF-16 offset; `None` or past the end is the end.
fn caret_index(chars: &[char], caret: Option<u32>) -> usize {
    let Some(target) = caret.and_then(|caret| usize::try_from(caret).ok()) else {
        return chars.len();
    };
    let mut units = 0;
    for (index, c) in chars.iter().enumerate() {
        if units >= target {
            return index;
        }
        units += c.len_utf16();
    }
    chars.len()
}

/// A whitespace-separated chunk with its character span.
struct Word<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    /// First word of the text, of a line or after a sentence end: its capital proves nothing.
    sentence_start: bool,
}

fn words(text: &str) -> Vec<Word<'_>> {
    let mut out = Vec::new();
    let mut sentence_start = true;
    let mut open: Option<(usize, usize)> = None;
    // A space after the end closes the last word like any other.
    let chars = text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')));
    for (position, (byte, c)) in chars.enumerate() {
        if c.is_whitespace() {
            if let Some((start_byte, start)) = open.take() {
                let word = text.get(start_byte..byte).unwrap_or_default();
                out.push(Word {
                    text: word,
                    start,
                    end: position,
                    sentence_start,
                });
                sentence_start = ends_sentence(word);
            }
            if c == '\n' {
                sentence_start = true;
            }
        } else if open.is_none() {
            open = Some((byte, position));
        }
    }
    out
}

fn ends_sentence(word: &str) -> bool {
    word.trim_end_matches(['"', '\'', ')', ']', '”', '’'])
        .ends_with(['.', '?', '!'])
}

/// The word inside a chunk without surrounding punctuation or a possessive "'s"; `None` for
/// emails, handles, URLs and paths, and for chunks with code punctuation inside.
fn hint_word(chunk: &str) -> Option<&str> {
    if chunk.contains(['@', '/', '\\']) {
        return None;
    }
    let core = chunk.trim_matches(|c: char| !c.is_alphanumeric());
    let core = core
        .strip_suffix("'s")
        .or_else(|| core.strip_suffix("’s"))
        .unwrap_or(core);
    core.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '\'' | '’'))
        .then_some(core)
}

/// An acronym, a word with an internal capital, or a capitalised word that does not start a
/// sentence.
fn is_hint(word: &str, sentence_start: bool) -> bool {
    let chars = word.chars().count();
    if chars < HINT_MIN_CHARS || secret_like(word) {
        return false;
    }
    let letters: Vec<char> = word.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return false;
    }
    if letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()) {
        return chars <= ACRONYM_MAX_CHARS && !COMMON_ACRONYMS.contains(&word);
    }
    if word.chars().skip(1).any(char::is_uppercase) {
        return true;
    }
    !sentence_start && word.chars().next().is_some_and(char::is_uppercase)
}

/// Too long, too many digits, or case flipping like a random key.
fn secret_like(token: &str) -> bool {
    let chars: Vec<char> = token.chars().collect();
    let digits = chars.iter().filter(|c| c.is_ascii_digit()).count();
    let flips = chars
        .windows(2)
        .filter(|pair| match pair {
            [a, b] => {
                a.is_alphabetic() && b.is_alphabetic() && a.is_uppercase() != b.is_uppercase()
            }
            _ => false,
        })
        .count();
    chars.len() > CONTEXT_TERM_MAX_CHARS
        || digits >= SECRET_MIN_DIGITS
        || (digits > 0 && chars.len() > DIGIT_WORD_MAX_CHARS)
        || (chars.len() >= RANDOM_CASE_MIN_CHARS && flips * 3 > chars.len())
}

fn is_code_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '-' | '/' | '\\')
}

/// The file name a chunk ends in (`src/main.rs` gives `main.rs`), if its extension is a known
/// source or config one.
fn file_name(chunk: &str) -> Option<&str> {
    let chunk = chunk.trim_matches(|c: char| !is_code_char(c));
    let name = chunk
        .rsplit(['/', '\\'])
        .next()?
        .trim_end_matches(['.', '-']);
    let (stem, extension) = name.rsplit_once('.')?;
    if !CODE_EXTENSIONS.contains(&extension) || secret_like(name) {
        return None;
    }
    if stem.is_empty() {
        return Some(name);
    }
    let stem_ok = stem.chars().count() >= 2
        && stem.chars().any(char::is_alphabetic)
        && stem
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'));
    stem_ok.then_some(name)
}

/// The file name, or the identifiers, in one chunk of code text.
fn code_terms(chunk: &str) -> Vec<&str> {
    if let Some(name) = file_name(chunk) {
        return vec![name];
    }
    chunk
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .split(['.', '-'])
        .map(|part| part.trim_matches('_'))
        .filter(|part| is_identifier(part))
        .collect()
}

/// snake_case or SCREAMING_CASE (an inner underscore), or camelCase and PascalCase (lowercase
/// with an inner capital). Plain words and all-caps acronyms are not identifiers here.
fn is_identifier(part: &str) -> bool {
    let chars = part.chars().count();
    let starts_with_letter = part.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if chars < IDENTIFIER_MIN_CHARS
        || !starts_with_letter
        || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        || secret_like(part)
    {
        return false;
    }
    if part.contains('_') {
        return true;
    }
    part.chars().any(|c| c.is_ascii_lowercase())
        && part.chars().skip(1).any(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hints_are_names_acronyms_and_brand_casing_nearest_the_end_first() {
        let text = "Meeting notes. Today we met Priya and Okafor at NASA about the iPhone launch.";
        assert_eq!(
            context_terms(text, None),
            vec!["iPhone", "NASA", "Okafor", "Priya"]
        );
    }

    #[test]
    fn hints_are_ordered_by_distance_from_the_caret() {
        let text = "Ask Alice to write this. Then Bob replied, and Carol agreed";
        assert_eq!(context_terms(text, Some(30)), vec!["Bob", "Carol", "Alice"]);
    }

    #[test]
    fn the_caret_is_a_utf16_offset_and_past_the_end_means_the_end() {
        let text = "😀😀 Zed then Alpha";
        assert_eq!(context_terms(text, Some(0)), vec!["Zed", "Alpha"]);
        assert_eq!(context_terms(text, Some(14)), vec!["Alpha", "Zed"]);
        assert_eq!(context_terms(text, Some(999)), vec!["Alpha", "Zed"]);
    }

    #[test]
    fn emails_urls_paths_numbers_and_secrets_are_left_out() {
        let access_key = format!("{}{}", "AKIA", "IOSFODNN7EXAMPLE");
        let text = format!(
            "Send Priya's file to bob@Example.com via https://Example.com/Path or \
             /Users/Ankur/Docs, code 123456 key {access_key} and aBcDeFgHiJkLmN OK, plus Raycast"
        );
        assert_eq!(context_terms(&text, None), vec!["Raycast", "Priya"]);
    }

    #[test]
    fn only_the_text_around_the_caret_is_read() {
        let text = format!("Zanzibar {}Quokka", "word ".repeat(1_000));
        assert_eq!(context_terms(&text, None), vec!["Quokka"]);
    }

    #[test]
    fn hints_stay_within_the_count_and_character_caps() {
        let names: Vec<String> = (b'a'..=b'z')
            .map(|letter| format!("Q{}", char::from(letter).to_string().repeat(20)))
            .collect();
        let text = format!("and {}", names.join(" and "));
        let hints = context_terms(&text, None);
        assert!(!hints.is_empty());
        assert!(hints.len() < names.len());
        assert!(hints.len() <= CONTEXT_MAX_TERMS);
        assert!(hints.iter().map(|hint| hint.chars().count()).sum::<usize>() <= CONTEXT_MAX_CHARS);
    }

    #[test]
    fn ide_terms_are_file_names_then_identifiers_by_frequency() {
        let title = "● main.rs — simple-voice — Visual Studio Code";
        let text = "use crate::features::llm; fn polish_prompt(userName: &str) { let \
                    MAX_RETRIES = 3; see src/features/llm.rs and /Users/ankur/secret/notes.md. \
                    this.userName.length; ContentView Hello hello __init__ ok }";
        assert_eq!(
            ide_terms(text, Some(title)),
            vec![
                "main.rs",
                "userName",
                "polish_prompt",
                "MAX_RETRIES",
                "llm.rs",
                "notes.md",
                "ContentView"
            ]
        );
    }

    #[test]
    fn ide_terms_skip_secret_looking_tokens() {
        let api_key = format!("{}_{}_{}", "sk", "live", "4eC39HqLyjWDarjtT1zdp7dc");
        let text = format!("token = {api_key}; hash a1b2c3d4e5f6; getHTTPResponseCode v2Client");
        assert_eq!(
            ide_terms(&text, None),
            vec!["getHTTPResponseCode", "v2Client"]
        );
    }

    #[test]
    fn ide_terms_stay_within_the_caps() {
        let text: Vec<String> = (0..200_u8)
            .map(|n| {
                let high = char::from(b'a' + n / 26);
                let low = char::from(b'a' + n % 26);
                format!("item_{high}{low}_value")
            })
            .collect();
        let terms = ide_terms(&text.join(" "), None);
        assert_eq!(terms.len(), CONTEXT_MAX_TERMS);
        assert!(terms.iter().map(|term| term.chars().count()).sum::<usize>() <= CONTEXT_MAX_CHARS);
    }
}
