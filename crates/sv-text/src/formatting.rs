// FilePath: crates/sv-text/src/formatting.rs
//! Deterministic clean-up of the raw transcript. It runs on every dictation, before and
//! independently of the LLM pass, and costs microseconds on a few hundred characters, so the
//! paste still lands right after release when the LLM pass is off or fails.
//!
//! Scripts without letter case (Devanagari) pass through untouched apart from whitespace.

use sv_domain::Style;

use crate::vocabulary::{Rule, Vocabulary};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    /// Rule applications that changed the text; feeds the "dictionary fixes" insight.
    pub rule_hits: u32,
}

pub fn format(text: &str, vocabulary: &Vocabulary, style: Style) -> Formatted {
    let collapsed = collapse_whitespace(text);
    if collapsed.is_empty() {
        return Formatted {
            text: collapsed,
            rule_hits: 0,
        };
    }

    let mut chars: Vec<char> = collapsed.chars().collect();
    let mut rule_hits = 0;
    for rule in &vocabulary.rules {
        if let Some(pattern) = Pattern::compile(rule) {
            let (rewritten, hits) = pattern.replace_all(&chars, &rule.to);
            chars = rewritten;
            rule_hits += hits;
        }
    }

    let mut chars = tighten_spacing(&chars);
    capitalize_pronoun_i(&mut chars);
    let mut chars = capitalize_sentences(&chars);
    if style == Style::Casual {
        drop_single_trailing_period(&mut chars);
    }

    Formatted {
        text: chars.into_iter().collect(),
        rule_hits,
    }
}

/// Collapses runs of spaces and tabs to one space and drops blank lines; line breaks the model
/// produced are kept because they start new sentences.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else {
            continue;
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(first);
        for word in words {
            out.push(' ');
            out.push_str(word);
        }
    }
    out
}

enum Token {
    Letter(char),
    Whitespace,
}

/// A literal phrase matched case-insensitively and tolerant of any whitespace run between its
/// words. Word frontiers apply only on alphanumeric ends, so "btw" never fires inside "btwn"
/// while a phrase like "c++" still matches.
struct Pattern {
    tokens: Vec<Token>,
    word_start: bool,
    word_end: bool,
}

impl Pattern {
    fn compile(rule: &Rule) -> Option<Pattern> {
        let phrase = rule.from.trim();
        let first = phrase.chars().next()?;
        let last = phrase.chars().next_back()?;
        let mut tokens = Vec::with_capacity(phrase.len());
        for c in phrase.chars() {
            if c.is_whitespace() {
                if !matches!(tokens.last(), Some(Token::Whitespace)) {
                    tokens.push(Token::Whitespace);
                }
            } else {
                tokens.push(Token::Letter(c));
            }
        }
        Some(Pattern {
            tokens,
            word_start: first.is_alphanumeric(),
            word_end: last.is_alphanumeric(),
        })
    }

    /// End index of a match starting at `start`, if any.
    fn match_at(&self, text: &[char], start: usize) -> Option<usize> {
        let preceded_by_word = start
            .checked_sub(1)
            .and_then(|i| text.get(i))
            .is_some_and(|c| c.is_alphanumeric());
        if self.word_start && preceded_by_word {
            return None;
        }
        let mut at = start;
        for token in &self.tokens {
            match token {
                Token::Letter(expected) => {
                    if !same_letter(*text.get(at)?, *expected) {
                        return None;
                    }
                    at += 1;
                }
                Token::Whitespace => {
                    if !text.get(at)?.is_whitespace() {
                        return None;
                    }
                    while text.get(at).is_some_and(|c| c.is_whitespace()) {
                        at += 1;
                    }
                }
            }
        }
        if self.word_end && text.get(at).is_some_and(|c| c.is_alphanumeric()) {
            return None;
        }
        Some(at)
    }

    /// Replaces every non-overlapping match, scanning left to right. Matches whose text
    /// already equals the replacement are not counted as hits.
    fn replace_all(&self, text: &[char], replacement: &str) -> (Vec<char>, u32) {
        let mut out = Vec::with_capacity(text.len());
        let mut hits = 0;
        let mut at = 0;
        while let Some(&c) = text.get(at) {
            match self.match_at(text, at) {
                Some(end) => {
                    let matched = text.get(at..end).unwrap_or_default();
                    if !matched.iter().copied().eq(replacement.chars()) {
                        hits += 1;
                    }
                    out.extend(replacement.chars());
                    at = end;
                }
                None => {
                    out.push(c);
                    at += 1;
                }
            }
        }
        (out, hits)
    }
}

fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

fn is_closing_punctuation(c: char) -> bool {
    matches!(c, ',' | '.' | ';' | ':' | '?' | '!')
}

fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '?' | '!')
}

/// Removes the space Whisper puts before punctuation ("the file ," → "the file,") and the
/// double or edge spaces an empty replacement rule leaves behind. Punctuation that starts a
/// word (".env", ".5") keeps its space.
fn tighten_spacing(text: &[char]) -> Vec<char> {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for (index, &c) in text.iter().enumerate() {
        if c == ' ' {
            if out.last().is_none_or(|prev| prev.is_whitespace()) {
                continue;
            }
            out.push(c);
            continue;
        }
        if c == '\n' {
            while out.last() == Some(&' ') {
                out.pop();
            }
        }
        if is_closing_punctuation(c) {
            let ends_word = text
                .get(index + 1)
                .is_none_or(|next| next.is_whitespace() || is_closing_punctuation(*next));
            if ends_word {
                while out.last() == Some(&' ') {
                    out.pop();
                }
            }
        }
        out.push(c);
    }
    while out.last() == Some(&' ') {
        out.pop();
    }
    out
}

/// Standalone English "i" → "I"; never inside a word.
fn capitalize_pronoun_i(text: &mut [char]) {
    for index in 0..text.len() {
        let before = index.checked_sub(1).and_then(|i| text.get(i)).copied();
        let after = text.get(index + 1).copied();
        let standalone =
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric);
        if let Some(c) = text.get_mut(index) {
            if *c == 'i' && standalone {
                *c = 'I';
            }
        }
    }
}

/// Uppercases the first letter of the text, of each line, and after `.`, `?` or `!` followed by
/// whitespace. A word with its own internal capitals (iPhone, macOS) keeps its brand casing.
fn capitalize_sentences(text: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len());
    let mut sentence_start = true;
    let mut after_terminator = false;
    for (index, &c) in text.iter().enumerate() {
        if c == '\n' {
            sentence_start = true;
            after_terminator = false;
            out.push(c);
            continue;
        }
        if c.is_whitespace() {
            if after_terminator {
                sentence_start = true;
            }
            out.push(c);
            continue;
        }
        if sentence_start && c.is_lowercase() && !has_internal_capital(text, index) {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        sentence_start = false;
        after_terminator = is_sentence_end(c);
    }
    out
}

fn has_internal_capital(text: &[char], word_start: usize) -> bool {
    text.iter()
        .skip(word_start + 1)
        .take_while(|c| c.is_alphanumeric())
        .any(|c| c.is_uppercase())
}

/// Casual style: a one-sentence message loses its closing period ("Sounds good."). Ellipses,
/// question marks and multi-sentence text are left as they are.
fn drop_single_trailing_period(text: &mut Vec<char>) {
    let Some((&last, body)) = text.split_last() else {
        return;
    };
    if last != '.' || body.is_empty() || body.last() == Some(&'.') {
        return;
    }
    let several_sentences = body.contains(&'\n')
        || body.windows(2).any(
            |pair| matches!(pair, [end, space] if is_sentence_end(*end) && space.is_whitespace()),
        );
    if !several_sentences {
        text.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sv_domain::{DictionaryEntry, DictionarySource};

    fn vocab(entries: &[(&str, Option<&str>)]) -> Vocabulary {
        let entries: Vec<DictionaryEntry> = entries
            .iter()
            .map(|(phrase, replacement)| DictionaryEntry {
                id: 0,
                phrase: (*phrase).to_owned(),
                replacement: replacement.map(str::to_owned),
                created_at: 0,
                source: DictionarySource::Manual,
            })
            .collect();
        Vocabulary::from_entries(&entries)
    }

    fn formal(text: &str, vocabulary: &Vocabulary) -> Formatted {
        format(text, vocabulary, Style::Formal)
    }

    #[test]
    fn longest_rule_wins_over_a_shorter_overlapping_rule() {
        let v = vocab(&[("status", Some("state")), ("git status", Some("gst"))]);
        let out = formal("git status and status", &v);
        assert_eq!(out.text, "Gst and state");
        assert_eq!(out.rule_hits, 2);
    }

    #[test]
    fn rules_match_whole_words_only() {
        let v = vocab(&[("btw", Some("by the way"))]);
        assert_eq!(formal("btwn us btw", &v).text, "Btwn us by the way");
        assert_eq!(formal("btwn us", &v).rule_hits, 0);
    }

    #[test]
    fn rules_match_case_insensitively_and_across_whitespace_runs() {
        let v = vocab(&[("dot env", Some(".env"))]);
        let out = formal("open the DOT   Env file", &v);
        assert_eq!(out.text, "Open the .env file");
        assert_eq!(out.rule_hits, 1);
    }

    #[test]
    fn symbol_phrases_match_without_word_frontiers_on_symbol_ends() {
        let v = vocab(&[("c++", Some("C++"))]);
        assert_eq!(formal("i like c++.", &v).text, "I like C++.");
    }

    #[test]
    fn internal_caps_words_snap_brand_casing_but_capitalised_words_do_not() {
        let v = vocab(&[("ArgoCD", None), ("Frontend", None)]);
        let out = formal("deploy with argocd to the frontend", &v);
        assert_eq!(out.text, "Deploy with ArgoCD to the frontend");
        assert_eq!(out.rule_hits, 1);
    }

    #[test]
    fn already_correct_brand_casing_is_not_counted_as_a_fix() {
        let v = vocab(&[("ArgoCD", None)]);
        assert_eq!(formal("we use ArgoCD", &v).rule_hits, 0);
    }

    #[test]
    fn no_space_before_closing_punctuation_but_leading_dots_keep_their_space() {
        let v = Vocabulary::default();
        assert_eq!(
            formal("the file , then . ok ?", &v).text,
            "The file, then. Ok?"
        );
        assert_eq!(formal("edit the .env file", &v).text, "Edit the .env file");
    }

    #[test]
    fn empty_replacement_leaves_no_double_space() {
        let v = vocab(&[("um", Some(""))]);
        assert_eq!(formal("so um we ship", &v).text, "So we ship");
    }

    #[test]
    fn capitalizes_after_sentence_ends_and_line_breaks() {
        let v = Vocabulary::default();
        let out = formal("done. next? yes! see file.txt\nnew line", &v);
        assert_eq!(out.text, "Done. Next? Yes! See file.txt\nNew line");
    }

    #[test]
    fn keeps_brand_casing_at_sentence_start() {
        let v = Vocabulary::default();
        assert_eq!(
            formal("iPhone is here. macOS too", &v).text,
            "iPhone is here. macOS too"
        );
    }

    #[test]
    fn standalone_i_becomes_capital_but_not_inside_words() {
        let v = Vocabulary::default();
        assert_eq!(
            formal("so i think i'm in it", &v).text,
            "So I think I'm in it"
        );
    }

    #[test]
    fn whitespace_collapses_and_blank_lines_drop() {
        let v = Vocabulary::default();
        assert_eq!(
            formal("  hello \t  world \n\n  again  ", &v).text,
            "Hello world\nAgain"
        );
        assert_eq!(formal("   ", &v).text, "");
    }

    #[test]
    fn devanagari_passes_through_untouched() {
        let v = Vocabulary::default();
        let text = "मैं आज घर जा रहा हूँ। फिर मिलते हैं";
        assert_eq!(formal(text, &v).text, text);
        assert_eq!(format(text, &v, Style::Casual).text, text);
    }

    #[test]
    fn casual_drops_the_period_of_a_single_sentence_only() {
        let v = Vocabulary::default();
        let casual = |text: &str| format(text, &v, Style::Casual).text;
        assert_eq!(casual("sounds good."), "Sounds good");
        assert_eq!(casual("sounds good. see you."), "Sounds good. See you.");
        assert_eq!(casual("wait..."), "Wait...");
        assert_eq!(casual("really?"), "Really?");
        assert_eq!(formal("sounds good.", &v).text, "Sounds good.");
    }
}
