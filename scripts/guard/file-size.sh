#!/usr/bin/env bash
# FilePath: scripts/guard/file-size.sh
# Enforces the repository's flat file-size ceiling across the source extensions
# scripts/guard/file-path-headers.sh also covers (`.rs`, `.ts`, `.tsx`, `.js`, `.css`, `.sql`,
# `.swift`, `.toml`, `.sh`, `.just`, `.yml`, `.yaml`, `.rb`, the root `justfile`).
# A file that grows past this is a module that wants to be split, not a file that wants a
# bigger limit. There are no per-language exemptions: every tracked file in the extension
# list above answers to the same number.
#
# The number counts PRODUCTION lines. In a `.rs` file every line from the first
# `#[cfg(test)]` attribute onward is test code and is not counted, which is why AGENTS.md
# says "test code excluded".
# Tests live inline at the bottom of the file they cover (scripts/guard/inline-tests.sh
# enforces that), so a well-tested module is never pushed to split against its grain by the
# size of its own test suite. Non-Rust files have no test boundary and count every line.
#
# The ceiling lives in quality.toml's [file_size].all, read via scripts/lib/quality.sh
# (no TOML parser dependency - this runs in a git hook), and scripts/guard/ratchet.sh is
# what stops that number from ever moving looser. A missing quality.toml or missing key is
# a guard failure, never a silent fallback.
#
# [file_size.exceptions] records debt against the ceiling, never a waiver (see quality.toml's
# header): a file not listed must meet the real ceiling; a listed file may exceed the real
# ceiling up to its own recorded number, but growing past THAT still fails; a listed file
# whose count has dropped back to/under the real ceiling fails too - the debt is paid off, so
# the entry is stale bookkeeping and must be deleted.
#
# Line counts come from ONE batched awk pass over the whole `git ls-files -z` list (piped
# through `xargs -0`, which may split the list into several awk invocations for very large
# repos - each invocation only ever sees a disjoint slice of files, so the per-file counts
# it prints never collide) instead of a `wc -l` subprocess per file. A cheap builtin
# `[ -f ]` filter runs ahead of that pass so a file `git ls-files` still lists but the
# worktree has since deleted (uncommitted `git rm`, a concurrent editor mid-change) cannot
# make awk itself fail to open it, nor silently vanish from the scanned count without a
# trace: the filter runs before the count is taken, and an empty result after filtering is
# itself a guard failure (see below), never a silent all-clear.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=scripts/lib/quality.sh disable=SC1091
source "${root}/scripts/lib/quality.sh"

baseline=$(quality_limit file_size all) || exit 1

cd "$root"

patterns=('*.rs' '*.ts' '*.tsx' '*.js' '*.css' '*.sql' '*.swift' '*.toml' '*.sh' '*.just'
    '*.yml' '*.yaml' '*.rb' 'justfile')

total=$(git ls-files -z -- "${patterns[@]}" | tr -cd '\0' | wc -c | tr -d ' ')
if [ "$total" -eq 0 ]; then
    echo "file-size: 0 tracked files matched the extension list - check git ls-files patterns" >&2
    exit 1
fi

filtered=$(mktemp)
trap 'rm -f "$filtered"' EXIT
git ls-files -z -- "${patterns[@]}" | while IFS= read -r -d '' f; do
    [ -f "$f" ] && printf '%s\0' "$f"
done >"$filtered"
if [ ! -s "$filtered" ]; then
    echo "file-size: ${total} tracked files matched but none exist in the working tree - fail-open, not a clean pass" >&2
    exit 1
fi

counts=$(xargs -0 awk '
    FNR == 1 { skip = 0; n[FILENAME] += 0 }
    FILENAME ~ /\.rs$/ && /^[[:space:]]*#\[cfg\(test\)\]/ { skip = 1 }
    skip { next }
    { n[FILENAME]++ }
    END { for (f in n) print n[f], f }
' <"$filtered")
if [ -z "$counts" ]; then
    echo "file-size: internal error - candidate files existed but awk produced no line counts" >&2
    exit 1
fi

# Only pay the per-file `quality_exception` subprocess cost when there is actually an
# exceptions table to consult; the common case (this repo carries zero size debt) stays a
# single batched awk pass with no per-file overhead at all.
has_exceptions=$(awk '
    /^\[/ { in_table = ($0 == "[file_size.exceptions]"); next }
    in_table && /^[[:space:]]*"/ { print "y"; exit }
' "${SV_QUALITY_TOML:-${root}/quality.toml}")

fail=0
scanned=0
while read -r lines file; do
    [ -z "$file" ] && continue
    scanned=$((scanned + 1))
    exception=""
    if [ -n "$has_exceptions" ]; then
        exception=$(quality_exception file_size.exceptions "$file")
    fi
    if [ -n "$exception" ]; then
        if [ "$lines" -gt "$exception" ]; then
            echo "${file}:${lines}: file exceeds its quality.toml [file_size.exceptions] ceiling of ${exception} production lines (${lines} lines) - split it, or re-measure and lower the exception, never raise it"
            fail=1
        elif [ "$lines" -le "$baseline" ]; then
            echo "${file}:${lines}: file is back at/under the real ${baseline}-line ceiling but still has a [file_size.exceptions] entry (${exception}) - delete the stale entry in quality.toml"
            fail=1
        fi
    elif [ "$lines" -gt "$baseline" ]; then
        echo "${file}:${lines}: file exceeds the ${baseline}-line production limit (${lines} lines; Rust test code after #[cfg(test)] is not counted) - split it into modules, or record deliberate debt in [file_size.exceptions]"
        fail=1
    fi
done <<EOF
${counts}
EOF

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "file-size: OK (${scanned} files, ceiling ${baseline})"
