#!/usr/bin/env bash
# FilePath: scripts/guard/no-suppressions.sh
# Lint findings get fixed, never silenced. Fails when a tracked file carries a suppression:
#
#   .rs              `#[allow(`, `#![allow(`, `#[expect(`, `#![expect(`, and the
#                    `cfg_attr(..., allow(...))` / `cfg_attr(..., expect(...))` disguises
#   .ts .tsx .js     `eslint-disable` directives, `@ts-ignore`, `@ts-expect-error`, `@ts-nocheck`
#   .swift           `swiftlint:disable`
#
# The workspace lints already deny `allow_attributes`; this is the textual backstop for what the
# compiler cannot see (a disabled cfg, a macro body) and the only control for the TypeScript and
# Swift directives, which are comments. If a lint is wrong, change Cargo.toml, clippy.toml or
# eslint.config.js with a reason; never suppress a single call site. For Rust, whole-line `//`
# comments are excluded so prose may name the rule; the TypeScript and Swift directives are
# comments themselves, so those scans keep every match. Operates on `git ls-files -z` output.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

rs_files=()
ts_files=()
swift_files=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] || continue
    case "$f" in
        *.rs) rs_files+=("$f") ;;
        *.ts | *.tsx | *.js) ts_files+=("$f") ;;
        *.swift) swift_files+=("$f") ;;
    esac
done < <(git ls-files -z -- '*.rs' '*.ts' '*.tsx' '*.js' '*.swift')

if [ "${#rs_files[@]}" -eq 0 ] && [ "${#ts_files[@]}" -eq 0 ]; then
    echo "no-suppressions: internal error - no tracked source files; this guard measured nothing" >&2
    exit 1
fi

fail=0

# $1 = extended regex, $2 = message, $3 = "code" (drop whole-line // comments) or "comments"
# (keep every match), remaining args = files to scan.
scan() {
    local pattern="$1" message="$2" mode="$3" raw status matches
    shift 3
    [ "$#" -eq 0 ] && return 0
    status=0
    raw=$(grep -nE "$pattern" -- "$@") || status=$?
    if [ "$status" -ge 2 ]; then
        echo "no-suppressions: internal error - grep failed (exit ${status})" >&2
        exit 1
    fi
    [ "$status" -eq 0 ] || return 0
    if [ "$mode" = "code" ]; then
        matches=$(printf '%s\n' "$raw" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
    else
        matches="$raw"
    fi
    if [ -n "$matches" ]; then
        fail=1
        while IFS=: read -r file line _rest; do
            echo "${file}:${line}: ${message}"
        done <<<"$matches"
    fi
}

scan '#!?\[[[:space:]]*(allow|expect)[[:space:]]*\(' \
    "lint suppression attribute is forbidden - fix the lint, don't allow/expect it" \
    code ${rs_files[@]+"${rs_files[@]}"}
scan 'cfg_attr[[:space:]]*\(.*(allow|expect)[[:space:]]*\(' \
    "cfg_attr(..., allow/expect(...)) is a lint suppression in disguise - forbidden" \
    code ${rs_files[@]+"${rs_files[@]}"}
scan 'eslint-disable' \
    "eslint-disable directive is forbidden - fix the code or change eslint.config.js" \
    comments ${ts_files[@]+"${ts_files[@]}"}
scan '@ts-(ignore|expect-error|nocheck)' \
    "TypeScript compiler suppression directive is forbidden - fix the type error, don't hide it" \
    comments ${ts_files[@]+"${ts_files[@]}"}
scan 'swiftlint:disable' \
    "swiftlint:disable is forbidden - fix the code instead" \
    comments ${swift_files[@]+"${swift_files[@]}"}

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "no-suppressions: OK (${#rs_files[@]} .rs, ${#ts_files[@]} .ts/.tsx/.js, ${#swift_files[@]} .swift files scanned)"
