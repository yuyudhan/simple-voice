#!/usr/bin/env bash
# FilePath: scripts/guard/file-path-headers.sh
# Every tracked source file must open with a comment naming its own repo-relative path, right
# after the shebang line where one exists, so a file is self-identifying when it is read out of
# context (a diff hunk, a pasted snippet, a partial read):
#
#     // FilePath: <path>        .rs .ts .tsx .js .swift
#     /* FilePath: <path> */     .css (CSS has no line-comment syntax)
#     -- FilePath: <path>        .sql
#     # FilePath: <path>         .toml .sh .just .yml .yaml .rb, the root justfile, the git hooks,
#                                .gitignore, .prettierignore, .editorconfig
#     <!-- FilePath: <path> -->  .md
#
# `Package.swift` must keep `// swift-tools-version:` as its first line (SwiftPM reads it), so
# there the header is expected on line 2, exactly like a shebang.
#
# JSON has no comment syntax and is skipped, as are lockfiles, generated icons and the `.sqlx/`
# offline query cache (JSON). A header naming the wrong path is a violation, not just a missing
# one. Operates on `git ls-files` only; a tracked path missing from the working tree is skipped.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

fail=0
scanned=0

while IFS= read -r -d '' file; do
    case "$file" in
        .sqlx/* | */.sqlx/* | src-tauri/icons/* | src-tauri/gen/* | node_modules/* | target/*)
            continue
            ;;
    esac

    prefix=""
    suffix=""
    case "$file" in
        *.rs | *.ts | *.tsx | *.js | *.swift) prefix="// FilePath: " ;;
        *.css)
            prefix="/* FilePath: "
            suffix=" */"
            ;;
        *.sql) prefix="-- FilePath: " ;;
        *.md)
            prefix="<!-- FilePath: "
            suffix=" -->"
            ;;
        *.toml | *.sh | *.just | *.yml | *.yaml | *.rb) prefix="# FilePath: " ;;
        justfile | .githooks/* | .gitignore | .prettierignore | .editorconfig)
            prefix="# FilePath: "
            ;;
        *) continue ;;
    esac

    [ -f "$file" ] || continue
    scanned=$((scanned + 1))

    line1=""
    line2=""
    {
        IFS= read -r line1 || true
        IFS= read -r line2 || true
    } <"$file"

    expected="${prefix}${file}${suffix}"
    if [[ "$line1" == '#!'* || "$line1" == '// swift-tools-version'* ]]; then
        lineno=2
        actual="$line2"
    else
        lineno=1
        actual="$line1"
    fi

    if [ "$actual" != "$expected" ]; then
        echo "${file}:${lineno}: missing or incorrect FilePath header - expected \"${expected}\""
        fail=1
    fi
done < <(git ls-files -z)

if [ "$scanned" -eq 0 ]; then
    echo "file-path-headers: internal error - no candidate files matched; this guard measured nothing" >&2
    exit 1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "file-path-headers: OK (${scanned} files scanned)"
