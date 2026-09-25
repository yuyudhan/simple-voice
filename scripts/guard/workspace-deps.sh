#!/usr/bin/env bash
# FilePath: scripts/guard/workspace-deps.sh
# Every dependency of a member crate (src-tauri/Cargo.toml, crates/*/Cargo.toml) is declared
# once in the root [workspace.dependencies] table and consumed as `name.workspace = true` or
# `name = { workspace = true, ... }`. A version or path pinned at the crate level is how a
# workspace ends up with two copies of the same crate at different versions, so this guard
# flags every key in a `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` or
# `[target.<cfg>.*dependencies]` table that is not workspace-sourced, and every
# `[dependencies.<name>]`-style table (which cannot be read line by line; use the inline form).
# Continuation lines of a multi-line inline table (a `features = [` list) are not keys and are
# skipped. Operates on `git ls-files -z` output only.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

fail=0
scanned=0
files=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] && files+=("$f")
done < <(git ls-files -z -- 'src-tauri/Cargo.toml' 'crates/*/Cargo.toml')

if [ "${#files[@]}" -eq 0 ]; then
    echo "workspace-deps: internal error - no member Cargo.toml files are tracked" >&2
    exit 1
fi

for file in "${files[@]}"; do
    scanned=$((scanned + 1))
    in_dep_table=0
    lineno=0
    while IFS= read -r line || [ -n "$line" ]; do
        lineno=$((lineno + 1))
        trimmed="${line#"${line%%[![:space:]]*}"}"
        trimmed="${trimmed%"${trimmed##*[![:space:]]}"}"
        case "$trimmed" in
            \[dependencies\] | \[dev-dependencies\] | \[build-dependencies\] | \[target.*dependencies\])
                in_dep_table=1
                continue
                ;;
            \[dependencies.* | \[dev-dependencies.* | \[build-dependencies.* | \[target.*dependencies.*)
                echo "${file}:${lineno}: [<table>.<name>] dependency tables are not allowed - use name = { workspace = true }"
                fail=1
                in_dep_table=0
                continue
                ;;
            \[*)
                in_dep_table=0
                continue
                ;;
        esac
        [ "$in_dep_table" -eq 1 ] || continue
        [ -z "$trimmed" ] && continue
        [[ "$trimmed" == \#* ]] && continue
        # Only lines that start a key are dependencies; the rest continue an inline table.
        [[ "$trimmed" =~ ^[A-Za-z0-9_-]+(\.[A-Za-z0-9_-]+)?[[:space:]]*= ]] || continue

        if [[ "$trimmed" =~ ^[A-Za-z0-9_-]+\.workspace[[:space:]]*=[[:space:]]*true$ ]]; then
            continue
        fi
        if [[ "$trimmed" =~ ^[A-Za-z0-9_-]+[[:space:]]*=[[:space:]]*\{[[:space:]]*workspace[[:space:]]*=[[:space:]]*true ]]; then
            continue
        fi
        echo "${file}:${lineno}: dependency is not workspace-sourced - declare it in the root [workspace.dependencies] and use name.workspace = true"
        fail=1
    done <"$file"
done

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "workspace-deps: OK (${scanned} Cargo.toml files scanned)"
