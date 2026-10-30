// FilePath: crates/sv-text/src/sound_alike/phonetic.rs
//! How a written word sounds, coarsely: the key two spellings of the same spoken name share
//! ("Ujwal", "Udjval" and "Ujjwal" all become `ujval`; "Himanšu" and "Himanshu" `himansu`), and
//! the edit distance between two keys.
//!
//! The key folds Latin diacritics (Parakeet v3 writes Czech-style "š" for an Indian "sh"),
//! spelling variants of one sound (w/v, y/i, c/k/q, z/s, ph/f, dj/j), an `h` that only aspirates
//! the consonant before it (bh, dh, sh, th), and doubled letters.

/// The lowercase ASCII letters a non-ASCII `c` folds to, or `None` for a character that is not
/// a Latin letter (CJK, Devanagari, Cyrillic): such words are never matched.
fn fold(c: char) -> Option<&'static str> {
    let folded = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å'
        | 'Ā' | 'Ă' | 'Ą' => "a",
        'æ' | 'Æ' => "ae",
        'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => "c",
        'ď' | 'đ' | 'Ď' | 'Đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ė' | 'Ę'
        | 'Ě' => "e",
        'ğ' | 'Ğ' => "g",
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' | 'Į' | 'İ' => {
            "i"
        }
        'ł' | 'ľ' | 'ĺ' | 'Ł' | 'Ľ' | 'Ĺ' => "l",
        'ñ' | 'ń' | 'ň' | 'Ñ' | 'Ń' | 'Ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō'
        | 'Ő' => "o",
        'œ' | 'Œ' => "oe",
        'ř' | 'ŕ' | 'Ř' | 'Ŕ' => "r",
        'ś' | 'ş' | 'ș' | 'Ś' | 'Ş' | 'Ș' => "s",
        // Czech and Slovak write the "sh" sound as "š".
        'š' | 'Š' => "sh",
        'ß' => "ss",
        'ť' | 'ţ' | 'ț' | 'Ť' | 'Ţ' | 'Ț' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű'
        | 'Ų' => "u",
        'ý' | 'ÿ' | 'Ý' | 'Ÿ' => "y",
        'ź' | 'ż' | 'Ź' | 'Ż' => "z",
        'ž' | 'Ž' => "zh",
        _ => return None,
    };
    Some(folded)
}

/// `word` folded to lowercase ASCII letters, or `None` when it holds anything but Latin letters.
pub(crate) fn folded(word: &str) -> Option<String> {
    let mut out = String::with_capacity(word.len());
    for c in word.chars() {
        if c.is_ascii_alphabetic() {
            out.push(c.to_ascii_lowercase());
        } else {
            out.push_str(fold(c)?);
        }
    }
    (!out.is_empty()).then_some(out)
}

fn is_vowel(b: u8) -> bool {
    matches!(b, b'a' | b'e' | b'i' | b'o' | b'u')
}

/// The sound key of an already [`folded`] word.
pub(crate) fn key(folded: &str) -> Vec<u8> {
    let bytes = folded.as_bytes();
    let mut spelled = Vec::with_capacity(bytes.len() + 2);
    let mut i = 0;
    while i < bytes.len() {
        let pair = bytes.get(i..i + 2);
        let (sound, used): (&[u8], usize) = match pair {
            // "ch" is one sound distinct from "k"; it keeps its own letter.
            Some(b"ch") => (b"x", 2),
            Some(b"ph") => (b"f", 2),
            Some(b"dj") => (b"j", 2),
            Some(b"ck") => (b"k", 2),
            _ => match bytes[i] {
                b'x' => (b"ks", 1),
                b'w' => (b"v", 1),
                b'y' => (b"i", 1),
                b'c' | b'q' => (b"k", 1),
                b'z' => (b"s", 1),
                _ => (&bytes[i..=i], 1),
            },
        };
        spelled.extend_from_slice(sound);
        i += used;
    }

    let mut out: Vec<u8> = Vec::with_capacity(spelled.len());
    for (index, &b) in spelled.iter().enumerate() {
        let after_consonant =
            index > 0 && !is_vowel(spelled[index - 1]) && spelled[index - 1] != b'h';
        if b == b'h' && after_consonant {
            continue;
        }
        if out.last() == Some(&b) {
            continue;
        }
        out.push(b);
    }
    out
}

/// The distance from `pattern` to every prefix of `text`: entry `p` is the distance to
/// `text[..p]`. One pass serves every place a word can end inside a run-together span.
pub(crate) fn prefix_distances(pattern: &[u8], text: &[u8]) -> Vec<usize> {
    let mut row: Vec<usize> = (0..=text.len()).collect();
    for (i, &x) in pattern.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &y) in text.iter().enumerate() {
            let substitution = diagonal + usize::from(x != y);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(row[j + 1] + 1);
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_of(word: &str) -> String {
        String::from_utf8(key(&folded(word).unwrap())).unwrap()
    }

    fn distance(a: &[u8], b: &[u8]) -> usize {
        prefix_distances(a, b)[b.len()]
    }

    #[test]
    fn spellings_of_one_spoken_name_share_a_key() {
        for heard in ["Ujwal", "Udjval", "Ujval", "Ujjwal"] {
            assert_eq!(key_of(heard), "ujval", "{heard}");
        }
        for heard in ["Himanšu", "Hymanšu", "Himanshu"] {
            assert_eq!(key_of(heard), "himansu", "{heard}");
        }
        assert_eq!(key_of("Šilpa"), key_of("Shilpa"));
        assert_eq!(
            distance(key_of("Culky").as_bytes(), key_of("Kalki").as_bytes()),
            1
        );
        assert_eq!(key_of("Dhruv"), key_of("Druv"));
    }

    #[test]
    fn ch_stays_apart_from_k() {
        assert_ne!(key_of("chat"), key_of("kat"));
        assert_ne!(key_of("chat"), key_of("cat"));
    }

    #[test]
    fn non_latin_words_have_no_key() {
        assert_eq!(folded("Кибда"), None);
        assert_eq!(folded("कल"), None);
        assert_eq!(folded("v2"), None);
        assert_eq!(folded(""), None);
    }

    #[test]
    fn distances() {
        assert_eq!(distance(b"vedant", b"vedanta"), 1);
        assert_eq!(distance(b"", b"abc"), 3);
        assert_eq!(distance(b"angoragastia", b"ankuragastia"), 2);
        let prefixes = prefix_distances(b"ankur", b"ankuragastia");
        assert_eq!(prefixes[5], 0);
        assert_eq!(prefixes.len(), 13);
        assert_eq!(prefixes[0], 5);
    }
}
