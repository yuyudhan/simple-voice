// FilePath: crates/sv-text/src/learning.rs
//! Learning from corrections: finds the words the user changed in pasted text, builds the prompt
//! that asks the post-processing model which of them teach a spelling worth keeping, and reads
//! its answer. Only short word-level fixes are offered; a rewrite of the text says nothing about
//! how the recogniser should spell a word.

use std::time::Duration;

use crate::polish::PolishPrompt;

/// One word-level fix: what the recogniser wrote, what the user changed it to, and the edited
/// words around it so the model can tell a name from a rewording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    pub before: String,
    pub after: String,
    pub context: String,
}

/// Most corrections offered to the model from one dictation.
pub const MAX_CORRECTIONS: usize = 8;
/// The answer is a short list of numbers.
pub const LEARNING_MAX_TOKENS: u32 = 64;
/// Learning runs in the background after the paste, so it can wait longer than the polish pass.
pub const LEARNING_TIMEOUT: Duration = Duration::from_secs(10);

/// Texts longer than this are not diffed: the alignment is quadratic in the word count.
const MAX_TOKENS: usize = 600;
/// A hunk wider than this on either side is a rewording, not a misheard word or name.
const MAX_HUNK_TOKENS: usize = 3;
/// Edited words kept on each side of a hunk as its context.
const CONTEXT_TOKENS: usize = 4;
/// Longest word the dictionary accepts, in characters.
const MAX_WORD_CHARS: usize = 100;

const SYSTEM: &str = "The user dictated text. Speech recognition wrote each \"before\" and the \
user corrected it to \"after\". Choose the corrections that teach a word or name the recogniser \
should spell that way from now on.\n\
\n\
- Choose: names of people, places, companies, products and brands; technical terms, jargon, \
acronyms, unusual spellings or capitalisation.\n\
- Never choose: synonyms or rewording, grammar, tense or plural changes, ordinary dictionary \
words, numbers and dates, Hinglish romanisation variants (kya/kyaa), or anything where \"after\" \
is not clearly a better spelling of what was said.\n\
- Output only the numbers of the corrections to learn, separated by commas, or none. No other \
words.";

/// Worked examples, sent as a user turn and the assistant's reply: one where nothing is a word
/// to learn, and one where names and terms sit next to a rewording.
struct Shot {
    corrections: &'static [(&'static str, &'static str, &'static str)],
    reply: &'static str,
}

const SHOTS: [Shot; 3] = [
    Shot {
        corrections: &[
            ("big", "large", "we need a large table for the"),
            ("walk", "walked", "yesterday we walked to the park"),
        ],
        reply: "none",
    },
    Shot {
        corrections: &[
            (
                "cooper netties",
                "Kubernetes",
                "deploy the service on Kubernetes before the demo",
            ),
            ("quick", "fast", "it was a fast fix"),
            (
                "post gress",
                "Postgres",
                "move the orders table to Postgres next week",
            ),
        ],
        reply: "1, 3",
    },
    Shot {
        corrections: &[
            ("Brooklin", "Brooklyn", "the office in Brooklyn opens at"),
            ("colour", "color", "pick a color for the"),
        ],
        reply: "1",
    },
];

/// The text without the zero-width characters web editors insert around an edit (Gmail puts a
/// zero-width space before retyped words), which would otherwise end up inside a learned word.
/// Zero-width joiners stay: Devanagari spelling depends on them.
pub fn without_invisible(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{200B}' | '\u{2060}' | '\u{FEFF}'))
        .collect()
}

/// The word-level fixes between the pasted text and what it read after the user edited it, at
/// most [`MAX_CORRECTIONS`]. Empty when either text is blank or too long, or when so many words
/// changed that the edit is a rewrite rather than a correction.
pub fn corrections(pasted: &str, edited: &str) -> Vec<Correction> {
    let (pasted, edited) = (without_invisible(pasted), without_invisible(edited));
    let raw_before: Vec<&str> = pasted.split_whitespace().collect();
    let raw_after: Vec<&str> = edited.split_whitespace().collect();
    if raw_before.is_empty()
        || raw_after.is_empty()
        || raw_before.len() > MAX_TOKENS
        || raw_after.len() > MAX_TOKENS
    {
        return Vec::new();
    }
    let before: Vec<&str> = raw_before.iter().map(|token| strip(token)).collect();
    let after: Vec<&str> = raw_after.iter().map(|token| strip(token)).collect();
    let hunks = hunks(&before, &after);

    let changed: usize = hunks
        .iter()
        .map(|hunk| hunk.before_len().max(hunk.after_len()))
        .sum();
    if changed > (raw_before.len() / 2).max(4) {
        return Vec::new();
    }

    let mut found: Vec<Correction> = Vec::new();
    for hunk in &hunks {
        if found.len() == MAX_CORRECTIONS {
            break;
        }
        if hunk.before_len() == 0
            || hunk.after_len() == 0
            || hunk.before_len() > MAX_HUNK_TOKENS
            || hunk.after_len() > MAX_HUNK_TOKENS
        {
            continue;
        }
        let from = join(&before[hunk.before_start..hunk.before_end]);
        let to = join(&after[hunk.after_start..hunk.after_end]);
        if from.is_empty() || to.is_empty() || from == to || !to.chars().any(char::is_alphabetic) {
            continue;
        }
        if found
            .iter()
            .any(|seen| seen.before == from && seen.after == to)
        {
            continue;
        }
        let context_start = hunk.after_start.saturating_sub(CONTEXT_TOKENS);
        let context_end = hunk
            .after_end
            .saturating_add(CONTEXT_TOKENS)
            .min(raw_after.len());
        found.push(Correction {
            before: from,
            after: to,
            context: raw_after[context_start..context_end].join(" "),
        });
    }
    found
}

/// Chat messages asking the model which corrections to learn, the same for every backend.
pub fn learning_prompt(corrections: &[Correction]) -> PolishPrompt {
    PolishPrompt {
        system: SYSTEM.to_owned(),
        shots: SHOTS
            .iter()
            .map(|shot| {
                (
                    user_turn(shot.corrections.iter().copied()),
                    shot.reply.to_owned(),
                )
            })
            .collect(),
        user: user_turn(corrections.iter().map(|fix| {
            (
                fix.before.as_str(),
                fix.after.as_str(),
                fix.context.as_str(),
            )
        })),
    }
}

/// One numbered line per correction, so the answer can name them by number.
fn user_turn<'a>(lines: impl Iterator<Item = (&'a str, &'a str, &'a str)>) -> String {
    lines
        .enumerate()
        .map(|(index, (from, to, context))| {
            format!(
                "{}. \"{from}\" -> \"{to}\" (in: \"{context}\")",
                index.saturating_add(1)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The words to learn from the model's answer. `finished` is false when the model stopped for
/// any reason other than completing its answer; a cut-off list is not trusted. Numbers that name
/// no correction are ignored, so a chatty or malformed answer learns at most what it names.
pub fn accept_learning(corrections: &[Correction], output: &str, finished: bool) -> Vec<String> {
    if !finished {
        return Vec::new();
    }
    let mut chosen: Vec<usize> = Vec::new();
    for number in output
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|digits| digits.parse::<usize>().ok())
    {
        if (1..=corrections.len()).contains(&number) && !chosen.contains(&number) {
            chosen.push(number);
        }
    }
    chosen
        .into_iter()
        .filter_map(|number| corrections.get(number.saturating_sub(1)))
        .map(|fix| fix.after.clone())
        .filter(|word| word.chars().count() <= MAX_WORD_CHARS)
        .collect()
}

/// A run of consecutive non-matching alignment steps: `before[before_start..before_end]` became
/// `after[after_start..after_end]`. Either side may be empty (a pure insertion or deletion).
struct Hunk {
    before_start: usize,
    before_end: usize,
    after_start: usize,
    after_end: usize,
}

impl Hunk {
    fn before_len(&self) -> usize {
        self.before_end.saturating_sub(self.before_start)
    }

    fn after_len(&self) -> usize {
        self.after_end.saturating_sub(self.after_start)
    }
}

/// Aligns the two token lists with a word-level Levenshtein distance and groups the edits
/// between matching words into hunks, in text order.
fn hunks(before: &[&str], after: &[&str]) -> Vec<Hunk> {
    let width = after.len().saturating_add(1);
    let mut cost = vec![0_usize; before.len().saturating_add(1).saturating_mul(width)];
    let at = |i: usize, j: usize| i.saturating_mul(width).saturating_add(j);
    for i in 0..=before.len() {
        for j in 0..=after.len() {
            cost[at(i, j)] = if i == 0 {
                j
            } else if j == 0 {
                i
            } else {
                let diagonal = cost[at(i - 1, j - 1)]
                    .saturating_add(usize::from(before[i - 1] != after[j - 1]));
                let deletion = cost[at(i - 1, j)].saturating_add(1);
                let insertion = cost[at(i, j - 1)].saturating_add(1);
                diagonal.min(deletion).min(insertion)
            };
        }
    }

    // Walk back from the end; `matched` marks each aligned pair of equal words, which split hunks.
    let mut found: Vec<Hunk> = Vec::new();
    let mut open: Option<Hunk> = None;
    let (mut i, mut j) = (before.len(), after.len());
    while i > 0 || j > 0 {
        let here = cost[at(i, j)];
        let matched =
            i > 0 && j > 0 && before[i - 1] == after[j - 1] && here == cost[at(i - 1, j - 1)];
        if matched {
            found.extend(open.take());
            i -= 1;
            j -= 1;
            continue;
        }
        let hunk = open.get_or_insert(Hunk {
            before_start: i,
            before_end: i,
            after_start: j,
            after_end: j,
        });
        if i > 0 && j > 0 && here == cost[at(i - 1, j - 1)].saturating_add(1) {
            i -= 1;
            j -= 1;
        } else if i > 0 && here == cost[at(i - 1, j)].saturating_add(1) {
            i -= 1;
        } else {
            j -= 1;
        }
        hunk.before_start = i;
        hunk.after_start = j;
    }
    found.extend(open);
    found.reverse();
    found
}

/// A token without the punctuation that belongs to the sentence around it, so "world," matches
/// "world". Symbols that are part of a term stay: the dot of ".env", the "#" of "C#".
fn strip(token: &str) -> &str {
    token
        .trim_start_matches(|c| is_opening(c) || is_unicode_punctuation(c))
        .trim_end_matches(|c| is_closing(c) || is_unicode_punctuation(c))
}

fn is_opening(c: char) -> bool {
    matches!(c, '"' | '\'' | '(' | '[' | '{' | '<' | '`' | '*' | '_')
}

fn is_closing(c: char) -> bool {
    matches!(
        c,
        '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | ')' | ']' | '}' | '>' | '`' | '*' | '_'
    )
}

fn is_unicode_punctuation(c: char) -> bool {
    matches!(
        c,
        '\u{2010}'..='\u{2027}'
            | '\u{2030}'..='\u{205E}'
            | '\u{00A1}'
            | '\u{00A7}'
            | '\u{00AB}'
            | '\u{00B6}'
            | '\u{00B7}'
            | '\u{00BB}'
            | '\u{00BF}'
            | '\u{0964}'
            | '\u{0965}'
            | '\u{3001}'..='\u{3003}'
            | '\u{3008}'..='\u{3011}'
            | '\u{FF01}'..='\u{FF0F}'
            | '\u{FF1A}'..='\u{FF1F}'
    )
}

/// Stripped tokens joined by single spaces; tokens that were only punctuation drop out.
fn join(tokens: &[&str]) -> String {
    tokens
        .iter()
        .filter(|token| !token.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(before: &str, after: &str) -> (String, String) {
        (before.to_owned(), after.to_owned())
    }

    fn pairs(pasted: &str, edited: &str) -> Vec<(String, String)> {
        corrections(pasted, edited)
            .into_iter()
            .map(|fix| (fix.before, fix.after))
            .collect()
    }

    fn fix(before: &str, after: &str) -> Correction {
        Correction {
            before: before.to_owned(),
            after: after.to_owned(),
            context: String::new(),
        }
    }

    #[test]
    fn a_respelled_word_is_found_with_its_context() {
        let found = corrections(
            "we should ask marco about the budget tomorrow morning please",
            "we should ask Marko about the budget tomorrow morning please",
        );
        assert_eq!(
            found,
            vec![Correction {
                before: "marco".to_owned(),
                after: "Marko".to_owned(),
                context: "we should ask Marko about the budget tomorrow".to_owned(),
            }]
        );
    }

    #[test]
    fn a_capitalisation_fix_counts_and_surrounding_punctuation_does_not() {
        assert_eq!(
            pairs(
                "deploy it with argocd, then check",
                "deploy it with ArgoCD. then check"
            ),
            vec![pair("argocd", "ArgoCD")]
        );
    }

    #[test]
    fn a_punctuation_only_change_is_ignored() {
        assert!(corrections("hello world how are you", "hello, world! how are you?").is_empty());
    }

    #[test]
    fn pure_insertions_and_deletions_are_ignored() {
        let shorter = "send the file to the team";
        let longer = "send the new file to the team";
        assert!(corrections(shorter, longer).is_empty());
        assert!(corrections(longer, shorter).is_empty());
    }

    #[test]
    fn a_multi_word_hunk_is_one_correction() {
        assert_eq!(
            pairs(
                "we run it on cooper netties in production",
                "we run it on Kubernetes in production"
            ),
            vec![pair("cooper netties", "Kubernetes")]
        );
        assert_eq!(
            pairs(
                "the gate hub actions build failed",
                "the GitHub Actions build failed"
            ),
            vec![pair("gate hub actions", "GitHub Actions")]
        );
    }

    #[test]
    fn a_rewrite_yields_nothing() {
        assert!(corrections(
            "the quick brown fox jumps over the lazy dog",
            "a slow red cat sits under one sleepy bird"
        )
        .is_empty());
    }

    #[test]
    fn a_hunk_wider_than_three_words_is_ignored() {
        assert!(corrections(
            "one two three alpha beta gamma delta eight nine ten eleven twelve",
            "one two three north south east west eight nine ten eleven twelve"
        )
        .is_empty());
    }

    #[test]
    fn a_numeric_replacement_is_ignored() {
        assert!(corrections("meet at twenty past five", "meet at 20 past five").is_empty());
    }

    #[test]
    fn zero_width_spaces_never_reach_a_learned_word() {
        assert_eq!(
            pairs(
                "I really like your specs",
                "I really like your \u{200B}Specks"
            ),
            vec![pair("specs", "Specks")]
        );
        assert!(corrections("see you soon", "see you \u{200B}soon").is_empty());
    }

    #[test]
    fn symbols_that_belong_to_a_term_are_kept() {
        assert_eq!(
            pairs("load the dot env file, then", "load the .env file, then"),
            vec![pair("dot env", ".env")]
        );
        assert_eq!(
            pairs(
                "it is written in c sharp today.",
                "it is written in C# today."
            ),
            vec![pair("c sharp", "C#")]
        );
    }

    #[test]
    fn identical_fixes_are_offered_once_and_the_list_is_capped() {
        assert_eq!(
            pairs(
                "ask marco and then marco and marco again",
                "ask Marko and then Marko and Marko again"
            ),
            vec![pair("marco", "Marko")]
        );

        let pasted: Vec<String> = (0..10).map(|i| format!("w{i} keep keep")).collect();
        let edited: Vec<String> = (0..10).map(|i| format!("W{i}x keep keep")).collect();
        let found = pairs(&pasted.join(" "), &edited.join(" "));
        assert_eq!(found.len(), MAX_CORRECTIONS);
        assert_eq!(found.first(), Some(&pair("w0", "W0x")));
        assert_eq!(found.last(), Some(&pair("w7", "W7x")));
    }

    #[test]
    fn blank_text_yields_nothing() {
        assert!(corrections("   ", "ArgoCD").is_empty());
        assert!(corrections("argocd", "").is_empty());
    }

    #[test]
    fn the_answer_picks_corrections_by_number() {
        let fixes = [
            fix("marco", "Marko"),
            fix("big", "large"),
            fix("argocd", "ArgoCD"),
        ];
        assert_eq!(
            accept_learning(&fixes, "1, 3", true),
            vec!["Marko", "ArgoCD"]
        );
        assert_eq!(
            accept_learning(&fixes, "3,3 and 1", true),
            vec!["ArgoCD", "Marko"]
        );
        assert_eq!(accept_learning(&fixes, "0, 4, 2", true), vec!["large"]);
        assert!(accept_learning(&fixes, "none", true).is_empty());
        assert!(accept_learning(&fixes, "1, 3", false).is_empty());
    }

    #[test]
    fn an_overlong_word_is_never_learned() {
        let fixes = [fix("x", &"a".repeat(101))];
        assert!(accept_learning(&fixes, "1", true).is_empty());
    }

    #[test]
    fn the_prompt_numbers_each_correction() {
        let fixes = [
            Correction {
                before: "marco".to_owned(),
                after: "Marko".to_owned(),
                context: "ask Marko about it".to_owned(),
            },
            fix("argocd", "ArgoCD"),
        ];
        let prompt = learning_prompt(&fixes);
        assert_eq!(
            prompt.user,
            "1. \"marco\" -> \"Marko\" (in: \"ask Marko about it\")\n\
             2. \"argocd\" -> \"ArgoCD\" (in: \"\")"
        );
        assert!(prompt.shots.iter().any(|(_, reply)| reply == "none"));
        assert!(prompt
            .shots
            .iter()
            .all(|(user, _)| user.starts_with("1. \"")));
    }
}
