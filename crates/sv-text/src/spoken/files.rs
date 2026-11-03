// FilePath: crates/sv-text/src/spoken/files.rs
//! File names said with "dot": "package dot json" → `package.json`, "the dot env file" → `the
//! .env file`. A written extension in capitals is lowered ("package.JSON" → `package.json`,
//! "Next.JS" → `Next.js`), and a dotfile glued to the word before it ("the.env") gets its
//! space back. Only known extensions and dotfiles count, so "dot com" or "dot my i's" stay.

use super::{tokens, Edit};

/// File extensions a spoken name may end in.
const EXTENSIONS: &[&str] = &[
    "json", "md", "rs", "ts", "tsx", "js", "jsx", "mjs", "py", "rb", "go", "java", "kt", "swift",
    "yml", "yaml", "toml", "txt", "sh", "lock", "csv", "html", "css", "scss", "sql", "xml", "log",
    "ini", "cfg", "conf", "png", "jpg", "svg", "pdf", "zip",
];

/// Dotfiles said as "dot <name>".
const DOTFILES: &[&str] = &[
    "env",
    "gitignore",
    "npmrc",
    "nvmrc",
    "zshrc",
    "bashrc",
    "dockerignore",
    "editorconfig",
    "prettierrc",
    "eslintrc",
];

/// Words that are never a file's name ("the dot env" is `the .env`, not `the.env`).
const NOT_NAMES: &[&str] = &[
    "the", "a", "an", "this", "that", "my", "your", "our", "their", "his", "her", "its", "and",
    "or", "to", "in", "of", "on", "at", "for", "with", "from", "is", "it", "new", "open",
];

pub(crate) fn edits(text: &str) -> Vec<Edit> {
    let tokens = tokens(text);
    let mut edits = Vec::new();
    let mut at = 0;
    while at < tokens.len() {
        let token = &tokens[at];
        let next = |offset: usize| tokens.get(at + offset).map(|t| t.lower.as_str());
        let open = |index: usize| tokens.get(index).is_some_and(|t| !t.closed);
        // "package dot json"
        if open(at)
            && open(at + 1)
            && next(1) == Some("dot")
            && next(2).is_some_and(|ext| EXTENSIONS.contains(&ext))
            && is_name(&token.lower)
        {
            let ext = &tokens[at + 2];
            edits.push(Edit {
                start: token.start,
                end: ext.end,
                with: format!("{}.{}", token.core, ext.lower),
            });
            at += 3;
            continue;
        }
        // "dot env"
        if token.lower == "dot" && open(at) && next(1).is_some_and(|name| DOTFILES.contains(&name))
        {
            let name = &tokens[at + 1];
            edits.push(Edit {
                start: token.start,
                end: name.end,
                with: format!(".{}", name.lower),
            });
            at += 2;
            continue;
        }
        if let Some(with) = written_name(token.core) {
            edits.push(Edit {
                start: token.start,
                end: token.end,
                with,
            });
        }
        at += 1;
    }
    edits
}

fn is_name(word: &str) -> bool {
    !NOT_NAMES.contains(&word)
        && word.chars().next().is_some_and(char::is_alphanumeric)
        && word
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// A written name needing a fix: an extension in capitals, or a dotfile glued to a word.
fn written_name(word: &str) -> Option<String> {
    let (name, ext) = word.rsplit_once('.')?;
    let lower = ext.to_lowercase();
    if DOTFILES.contains(&lower.as_str()) && NOT_NAMES.contains(&name.to_lowercase().as_str()) {
        return Some(format!("{name} .{lower}"));
    }
    let shouted = ext.len() > 1 && ext.chars().all(|c| c.is_ascii_uppercase());
    (shouted && !name.is_empty() && EXTENSIONS.contains(&lower.as_str()))
        .then(|| format!("{name}.{lower}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(text: &str) -> String {
        super::super::splice(text, edits(text))
    }

    #[test]
    fn spoken_and_miswritten_file_names_are_fixed() {
        for (said, written) in [
            (
                "Put the key in the dot env file and update package dot json",
                "Put the key in the .env file and update package.json",
            ),
            (
                "Put the key in the.env file and update package.JSON.",
                "Put the key in the .env file and update package.json.",
            ),
            ("The Next.JS app", "The Next.js app"),
            ("open main dot rs", "open main.rs"),
        ] {
            assert_eq!(files(said), written, "{said}");
        }
    }

    #[test]
    fn other_dots_stay() {
        for text in [
            "Go to sienzo dot com",
            "I'm at home and dot my i's.",
            "Use Node.js and ASP.NET.",
            "The U.S. office",
            "Open the dot, then the file.",
        ] {
            assert_eq!(files(text), text, "{text}");
        }
    }
}
