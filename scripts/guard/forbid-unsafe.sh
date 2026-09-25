#!/usr/bin/env bash
# FilePath: scripts/guard/forbid-unsafe.sh
# Every Rust crate in this workspace forbids `unsafe`; code that needs Objective-C or C APIs
# lives in the Swift engine helper instead (requirements Q-3, Q-6). Four assertions, each
# closing a way the rule could silently stop applying:
#
#   1. The root Cargo.toml declares `unsafe_code = "forbid"` under [workspace.lints.rust].
#   2. Every member manifest (src-tauri/Cargo.toml, crates/*/Cargo.toml) carries a [lints]
#      table that is exactly `workspace = true`; a crate without it opts out of the workspace
#      lints and cargo does not warn.
#   3. Every crate root (lib.rs, main.rs and bin/*.rs under src-tauri/src and crates/*/src)
#      declares `#![forbid(unsafe_code)]` among its leading inner attributes, so the rule is
#      visible in the file a reader opens first.
#   4. No tracked `.rs` file contains the `unsafe` keyword outside a whole-line comment - a
#      textual backstop for code the compiler never sees (a disabled cfg, a macro body).
#
# Operates on `git ls-files -z` output only; tracked paths missing from the working tree are
# skipped.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

fail=0

# --- 1: [workspace.lints.rust] unsafe_code = "forbid" -----------------------------------------
if ! awk '
    /^\[/ { in_section = ($0 == "[workspace.lints.rust]"); next }
    in_section && /^[[:space:]]*unsafe_code[[:space:]]*=[[:space:]]*"forbid"/ { found = 1 }
    END { exit found ? 0 : 1 }
' Cargo.toml; then
    echo "Cargo.toml: [workspace.lints.rust] must set unsafe_code = \"forbid\""
    fail=1
fi

# --- 2: every member manifest inherits the workspace lints ------------------------------------
manifests=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] && manifests+=("$f")
done < <(git ls-files -z -- 'src-tauri/Cargo.toml' 'crates/*/Cargo.toml')

if [ "${#manifests[@]}" -eq 0 ]; then
    echo "forbid-unsafe: internal error - no member Cargo.toml files are tracked" >&2
    exit 1
fi

for manifest in "${manifests[@]}"; do
    verdict=$(awk '
        /^[[:space:]]*\[/ {
            in_lints = ($0 ~ /^[[:space:]]*\[lints\][[:space:]]*$/)
            if (in_lints) saw = 1
            next
        }
        in_lints && /^[[:space:]]*(#|$)/ { next }
        in_lints {
            line = $0
            gsub(/[[:space:]]/, "", line)
            if (line == "workspace=true") ok = 1
            else extra = 1
        }
        END {
            if (!saw) print "missing"
            else if (!ok || extra) print "custom"
            else print "ok"
        }
    ' "$manifest")
    case "$verdict" in
        missing)
            echo "${manifest}: missing [lints] table - add [lints] / workspace = true"
            fail=1
            ;;
        custom)
            echo "${manifest}: [lints] must contain exactly workspace = true"
            fail=1
            ;;
    esac
done

# --- 3: crate roots declare #![forbid(unsafe_code)] --------------------------------------------
roots=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] && roots+=("$f")
done < <(git ls-files -z -- \
    'src-tauri/src/lib.rs' 'src-tauri/src/main.rs' 'src-tauri/src/bin/*.rs' \
    'crates/*/src/lib.rs' 'crates/*/src/main.rs' 'crates/*/src/bin/*.rs')

if [ "${#roots[@]}" -eq 0 ]; then
    echo "forbid-unsafe: internal error - no crate root files are tracked" >&2
    exit 1
fi

for file in "${roots[@]}"; do
    # Scan the leading block of comments, blank lines and inner attributes; the first line
    # that is none of those ends the header, and the attribute must have appeared by then.
    if ! awk '
        /^[[:space:]]*$/ || /^[[:space:]]*\/\// { next }
        /^[[:space:]]*#!\[forbid\(unsafe_code\)\][[:space:]]*$/ { found = 1; exit }
        /^[[:space:]]*#!\[/ { next }
        { exit }
        END { exit found ? 0 : 1 }
    ' "$file"; then
        echo "${file}: crate root must declare #![forbid(unsafe_code)] before its first item"
        fail=1
    fi
done

# --- 4: no `unsafe` keyword in any tracked Rust source ---------------------------------------
rs_files=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] && rs_files+=("$f")
done < <(git ls-files -z -- '*.rs')

if [ "${#rs_files[@]}" -gt 0 ]; then
    status=0
    raw=$(grep -nE '(^|[^A-Za-z0-9_])unsafe([^A-Za-z0-9_]|$)' -- "${rs_files[@]}") || status=$?
    if [ "$status" -ge 2 ]; then
        echo "forbid-unsafe: internal error - grep failed (exit ${status})" >&2
        exit 1
    fi
    matches=$(printf '%s\n' "$raw" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
    if [ -n "$matches" ]; then
        while IFS=: read -r file line _rest; do
            [ -n "$file" ] || continue
            echo "${file}:${line}: the unsafe keyword is forbidden - move the code into the Swift engine helper"
        done <<<"$matches"
        fail=1
    fi
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "forbid-unsafe: OK (${#manifests[@]} manifests, ${#roots[@]} crate roots, ${#rs_files[@]} .rs files)"
