// FilePath: crates/sv-text/src/spoken/email.rs
//! Spoken email addresses: a local part, an at sign said as "at", "at the rate (of)", "at
//! rate" or "at sign", then words joined by "dot" that end in a known domain ending, written
//! lowercase without spaces: "ankit dot mehra at the rate sienzo dot in" →
//! `ankit.mehra@sienzo.in`. Without a domain ending there is no address, so "meet at the
//! office" stays. A plain "at" needs a local part that is no ordinary word ("we are at
//! google dot com" stays), unless the dictionary knows it.
//!
//! Inside an address every word is matched against the dictionary's words, ordinary ones
//! included, because the speech model hears an unfamiliar domain as English ("science dot
//! com" for `sienzo.com`). An address close to one the dictionary holds is written as it.

use super::{tokens, Edit, Token};
use crate::sound_alike::{is_ordinary_word, sound_distance, sound_key};
use crate::vocabulary::Vocabulary;

/// Domain endings an address may end in.
const ENDINGS: &[&str] = &[
    "com", "in", "io", "org", "net", "co", "ai", "dev", "app", "uk",
];

/// Spoken at signs, longest first; the plain "at" is guarded separately.
const MARKERS: &[&[&str]] = &[
    &["at", "the", "rate", "of"],
    &["at", "the", "rate"],
    &["at", "the", "red"],
    &["at", "rate"],
    &["at", "sign"],
    &["at", "the", "sign"],
];

/// Mail providers whose names are never "corrected" to a dictionary word.
const PROVIDERS: &[&str] = &[
    "gmail",
    "googlemail",
    "google",
    "yahoo",
    "outlook",
    "hotmail",
    "live",
    "icloud",
    "me",
    "microsoft",
    "onmicrosoft",
    "apple",
    "proton",
    "protonmail",
    "zoho",
    "aol",
    "rediffmail",
];

/// Words that never join a domain label: they end the address instead.
const STOPS: &[&str] = &[
    "and", "or", "cc", "bcc", "to", "the", "for", "with", "from", "then", "but", "please",
];

/// Whether a dictionary entry is an email address (`name@domain.ending`).
pub(crate) fn is_address(phrase: &str) -> bool {
    let Some((local, domain)) = phrase.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !phrase.contains(char::is_whitespace)
        && domain
            .rsplit_once('.')
            .is_some_and(|(name, ending)| !name.is_empty() && ENDINGS.contains(&ending))
}

pub(crate) fn edits(text: &str, vocabulary: &Vocabulary) -> Vec<Edit> {
    let tokens = tokens(text);
    let mut edits = Vec::new();
    let mut at = 1;
    while at < tokens.len() {
        match address_at(&tokens, at, vocabulary) {
            Some((first, end, address)) => {
                let start = tokens[first].start;
                let stop = tokens[end - 1].end;
                if text[start..stop] != address {
                    edits.push(Edit {
                        start,
                        end: stop,
                        with: address,
                    });
                }
                at = end + 1;
            }
            None => at += 1,
        }
    }
    // A written address the model got almost right ("Ankit@sinzo.com") is fixed in place.
    for token in &tokens {
        if is_address(&token.lower) && !edits.iter().any(|e| e.start == token.start) {
            let (local, domain) = token.lower.split_once('@').unwrap_or_default();
            let locals: Vec<String> = local.split('.').map(str::to_owned).collect();
            let labels: Vec<String> = domain.split('.').map(str::to_owned).collect();
            let address = assemble(&locals, &labels, vocabulary);
            if address != token.core {
                edits.push(Edit {
                    start: token.start,
                    end: token.end,
                    with: address,
                });
            }
        }
    }
    edits.sort_by_key(|edit| edit.start);
    edits
}

/// The address whose at sign starts at `tokens[at]`: (first token, end token, address).
fn address_at(
    tokens: &[Token<'_>],
    at: usize,
    vocabulary: &Vocabulary,
) -> Option<(usize, usize, String)> {
    let marker = MARKERS
        .iter()
        .find(|marker| {
            marker
                .iter()
                .enumerate()
                .all(|(i, word)| tokens.get(at + i).is_some_and(|t| t.lower == *word))
        })
        .map_or(usize::from(tokens[at].lower == "at"), |marker| marker.len());
    if marker == 0 || tokens[at..at + marker - 1].iter().any(|t| t.closed) {
        return None;
    }
    let (first, locals) = local_part(tokens, at)?;
    // A plain "at" is mostly the preposition: its local part must look like a name (four
    // letters or more, no ordinary word) unless the dictionary has it.
    let plain = |word: &String| word.chars().count() < 4 || is_ordinary_word(word);
    if marker == 1 && locals.iter().any(plain) {
        let known = |word: &String| vocabulary.email_words.contains(word);
        if !locals.iter().all(known) {
            return None;
        }
    }
    let (end, labels) = domain(tokens, at + marker)?;
    Some((first, end, assemble(&locals, &labels, vocabulary)))
}

/// The words before the at sign: one word, or words joined by "dot", "underscore" or "dash".
fn local_part(tokens: &[Token<'_>], at: usize) -> Option<(usize, Vec<String>)> {
    let mut first = at.checked_sub(1)?;
    if !is_word(&tokens[first].lower) || tokens[first].closed {
        return None;
    }
    let mut parts = vec![tokens[first].lower.clone()];
    while first >= 2 {
        let joiner = match tokens[first - 1].lower.as_str() {
            "dot" => ".",
            "underscore" => "_",
            "dash" | "hyphen" => "-",
            _ => break,
        };
        let word = &tokens[first - 2];
        if !is_word(&word.lower) || word.closed || tokens[first - 1].closed {
            break;
        }
        parts.insert(0, format!("{}{joiner}", word.lower));
        first -= 2;
    }
    // Joiners stay glued to the word before them; `assemble` joins with ".", so keep them.
    let mut locals = parts
        .join("")
        .split('.')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    // "and ccpriya.sharma at ...": the model ran "cc" into the name.
    let after_and = first > 0 && tokens[first - 1].lower == "and";
    if let Some(name) = locals[0]
        .strip_prefix("cc")
        .filter(|n| after_and && n.len() >= 3)
    {
        locals[0] = format!("cc {name}");
    }
    Some((first, locals))
}

/// The labels after the at sign and the token index just past them. Words without a "dot"
/// between them run together ("apps sienzo" → `appssienzo`); a word may carry its own dots
/// ("sienzo.com") or a glued ending ("dotin").
fn domain(tokens: &[Token<'_>], from: usize) -> Option<(usize, Vec<String>)> {
    let mut labels: Vec<String> = vec![String::new()];
    let mut words_in_label = 0;
    let mut best: Option<(usize, Vec<String>)> = None;
    let mut index = from;
    while let Some(token) = tokens.get(index) {
        let word = token.lower.as_str();
        if word == "dot" {
            if labels.last().is_none_or(String::is_empty) {
                return best;
            }
            labels.push(String::new());
            words_in_label = 0;
        } else {
            if STOPS.contains(&word) || words_in_label == 3 || !is_domain_word(word) {
                return best;
            }
            let (head, glued) = match word.strip_prefix("dot") {
                Some(ending) if ENDINGS.contains(&ending) && words_in_label > 0 => {
                    (None, Some(ending))
                }
                _ => (Some(word), None),
            };
            for (i, part) in head.into_iter().flat_map(|w| w.split('.')).enumerate() {
                if i > 0 {
                    labels.push(String::new());
                }
                if let Some(label) = labels.last_mut() {
                    label.push_str(part);
                }
            }
            if let Some(ending) = glued {
                labels.push(ending.to_owned());
            }
            words_in_label += 1;
        }
        index += 1;
        let ends = labels.len() >= 2
            && labels.last().is_some_and(|l| ENDINGS.contains(&l.as_str()))
            && labels.iter().all(|l| !l.is_empty());
        if ends {
            best = Some((index, labels.clone()));
        }
        if token.closed {
            break;
        }
    }
    best
}

/// A local-part word, which may carry the dots the model wrote itself ("ankit.mehr").
fn is_word(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with('.')
        && !word.ends_with('.')
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn is_domain_word(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with('.')
        && !word.ends_with('.')
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-')
}

/// Joins the parts, each word snapped to a dictionary word it sounds like, then the whole
/// address snapped to a known address it is close to.
fn assemble(locals: &[String], labels: &[String], vocabulary: &Vocabulary) -> String {
    let words = &vocabulary.email_words;
    let local: Vec<String> = locals.iter().map(|part| snap(part, words)).collect();
    let last = labels.len().saturating_sub(1);
    let domain: Vec<String> = labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            if i == last || PROVIDERS.contains(&label.as_str()) {
                label.clone()
            } else {
                snap(label, words)
            }
        })
        .collect();
    let address = format!("{}@{}", local.join("."), domain.join("."));
    known(&address, &vocabulary.addresses).unwrap_or(address)
}

/// The dictionary word `part` sounds like: same first sound and at most one edit per two
/// sounds of the longer key. That is loose for prose but safe inside an address, where the
/// only candidates are the dictionary's own words. A part that already is one stays.
fn snap(part: &str, words: &[String]) -> String {
    if part.chars().count() < 3 || words.iter().any(|w| w == part) {
        return part.to_owned();
    }
    let Some(key) = sound_key(part) else {
        return part.to_owned();
    };
    let vowel = |c: Option<&u8>| c.is_some_and(|c| b"aeiou".contains(c));
    words
        .iter()
        .filter_map(|word| {
            let other = sound_key(word)?;
            let same_start =
                key.first() == other.first() || (vowel(key.first()) && vowel(other.first()));
            let distance = sound_distance(&other, &key);
            (same_start && distance * 2 <= key.len().max(other.len())).then_some((distance, word))
        })
        .min_by_key(|(distance, _)| *distance)
        .map_or_else(|| part.to_owned(), |(_, word)| word.clone())
}

/// The known address `address` is close to, closest first: with the same local part, at most
/// one edit per three characters of the domain (the speech model mangles an unfamiliar domain
/// far more than a name); otherwise one edit per four characters of the whole address.
fn known(address: &str, addresses: &[String]) -> Option<String> {
    let (local, domain) = address.split_once('@')?;
    addresses
        .iter()
        .filter_map(|known| {
            let (known_local, known_domain) = known.split_once('@')?;
            let close = if known_local == local {
                let distance = levenshtein(domain, known_domain);
                (distance * 3 <= domain.len().max(known_domain.len())).then_some(distance)
            } else {
                let distance = levenshtein(address, known);
                (distance * 4 <= address.len().max(known.len())).then_some(distance)
            };
            close.map(|distance| (distance, known))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, known)| known.clone())
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let substitution = diagonal + usize::from(x != *y);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use sv_domain::{DictionaryEntry, DictionarySource};

    use super::*;

    fn vocabulary(words: &[&str]) -> Vocabulary {
        let entries: Vec<DictionaryEntry> = (1..)
            .zip(words)
            .map(|(id, phrase)| DictionaryEntry {
                id,
                phrase: (*phrase).to_owned(),
                replacement: None,
                created_at: 0,
                source: DictionarySource::Manual,
            })
            .collect();
        Vocabulary::new(&entries, &[])
    }

    fn mail(text: &str, words: &[&str]) -> String {
        super::super::splice(text, edits(text, &vocabulary(words)))
    }

    const TEAM: &[&str] = &[
        "Ankit",
        "Kalvi",
        "Sienzo",
        "ankit@appssienzo.onmicrosoft.com",
        "ankit.mehra@sienzo.in",
    ];

    #[test]
    fn spoken_at_signs_and_dots_become_an_address() {
        assert_eq!(
            mail("Ankit at the rate sienzo dot com", TEAM),
            "ankit@sienzo.com"
        );
        assert_eq!(
            mail("Kalvi at rate sienzo dot com.", TEAM),
            "kalvi@sienzo.com."
        );
        assert_eq!(
            mail(
                "Send it to ankit at sienzo dot com and cc priya dot sharma at gmail dot com",
                TEAM
            ),
            "Send it to ankit@sienzo.com and cc priya.sharma@gmail.com"
        );
        assert_eq!(
            mail("write to support at sign acme dot co dot in today", &[]),
            "write to support@acme.co.in today"
        );
        assert_eq!(
            mail("mail rahul at the rate of example.in", &[]),
            "mail rahul@example.in"
        );
    }

    #[test]
    fn words_inside_an_address_snap_to_the_dictionary() {
        // Shaped like Parakeet's hearings of real recordings: the domain heard as English.
        assert_eq!(
            mail("Angkit at the red sciencey dot com.", TEAM),
            "ankit@sienzo.com."
        );
        assert_eq!(
            mail("Kalvi at the rate science.com", TEAM),
            "kalvi@sienzo.com"
        );
        assert_eq!(
            mail("My email is ankit dot mehr at sinsy dotin.", TEAM),
            "My email is ankit.mehra@sienzo.in."
        );
        // A provider stays as it is.
        assert_eq!(
            mail("kalvi at the rate gmail dot com", TEAM),
            "kalvi@gmail.com"
        );
    }

    #[test]
    fn a_close_known_address_is_written_as_known() {
        assert_eq!(
            mail(
                "The reset was done for ancore at the rate apps.sot on Microsoft.com.",
                TEAM
            ),
            "The reset was done for ankit@appssienzo.onmicrosoft.com."
        );
        assert_eq!(
            mail("Mail Ankit@sinzo.com now", TEAM),
            "Mail ankit@sienzo.com now"
        );
    }

    #[test]
    fn no_domain_ending_no_address() {
        for text in [
            "Let's meet at the office at five.",
            "Growth at the rate of five percent.",
            "We are at google dot com today.",
            "I'm at home dot com",
            "Look at this dot",
        ] {
            assert_eq!(mail(text, TEAM), text, "{text}");
        }
    }

    #[test]
    fn recognises_dictionary_addresses() {
        assert!(is_address("ankit@sienzo.com"));
        assert!(is_address("a.b@x.co"));
        assert!(!is_address("ankit@sienzo"));
        assert!(!is_address("@sienzo.com"));
        assert!(!is_address("ankit at sienzo.com"));
    }
}
