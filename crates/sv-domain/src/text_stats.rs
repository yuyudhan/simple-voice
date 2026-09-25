// FilePath: crates/sv-domain/src/text_stats.rs
//! Word statistics stored with every history row and summed by Insights.

/// Whitespace-separated words. Bullets and numbering markers count as words, matching what a
/// reader sees.
pub fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Word-level Levenshtein distance between the transcript and the pasted text, comparing words
/// case-insensitively with surrounding punctuation ignored, so "hello," → "Hello." is not a
/// correction but "gonna" → "going to" is. O(n·m) time, O(m) memory.
pub fn words_corrected(before: &str, after: &str) -> usize {
    let a: Vec<String> = before.split_whitespace().map(normalize).collect();
    let b: Vec<String> = after.split_whitespace().map(normalize).collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, word_a) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, word_b) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(word_a != word_b);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

fn normalize(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punctuation_and_case_are_not_corrections() {
        assert_eq!(words_corrected("so hello, world", "So hello world."), 0);
    }

    #[test]
    fn counts_substitutions_insertions_and_deletions() {
        assert_eq!(words_corrected("um I gonna go", "I am going to go"), 4);
        assert_eq!(words_corrected("", "one two"), 2);
        assert_eq!(words_corrected("one two", ""), 2);
    }

    #[test]
    fn counts_words_across_lines() {
        assert_eq!(word_count("For the release:\n1. Update\n2. Tag"), 7);
    }
}
