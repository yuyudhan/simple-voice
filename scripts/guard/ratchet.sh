#!/usr/bin/env bash
# FilePath: scripts/guard/ratchet.sh
# quality.toml holds every size ceiling this repo enforces as one number each, so a gate can
# never be "fixed" by editing the number away: this script is the enforcement. It diffs the
# working tree's quality.toml against the version committed at HEAD and fails if:
#
#   - the [file_size].all ceiling increased, or was removed (deleting a ceiling disables its
#     gate - the loosest possible move, so it fails exactly like raising it would);
#   - any [file_size.exceptions] entry was raised; or
#   - any new [file_size.exceptions] entry appeared at all - an
#     exception is debt with a ceiling, never permission (see quality.toml's own header), and
#     this tree is adopting the ratchet with zero debt, so it never gets to add any through
#     this guard.
#
# Tables and keys are hardcoded (not inferred from quality.toml's own shape) because "these
# are all ceilings" is a real, closed fact about this repo's schema that should not rest on a
# naming convention silently drifting out of sync with quality.toml.
set -euo pipefail

self="scripts/guard/ratchet.sh"
root="$(cd "$(dirname "$0")/../.." && pwd)"
quality_toml="${root}/quality.toml"

if [ ! -f "$quality_toml" ]; then
    echo "${self}: quality.toml not found at ${quality_toml}" >&2
    exit 1
fi

if ! git -C "$root" cat-file -e HEAD:quality.toml 2>/dev/null; then
    echo "ratchet: OK (quality.toml not yet committed at HEAD - this is the adoption commit)"
    exit 0
fi

prev_toml=$(mktemp)
fail_marker=$(mktemp)
trap 'rm -f "$prev_toml" "$fail_marker"' EXIT
git -C "$root" show HEAD:quality.toml >"$prev_toml"

# A single flat `key = value` line's integer value under `[table]`.
table_int() {
    awk -v want="[$1]" -v key="$2" '
        /^\[/ { in_table = ($0 == want); next }
        in_table && $1 == key && $2 == "=" { print $3; found = 1 }
        END { if (!found) exit 1 }
    ' "$3"
}

# Ceilings only ever fall. A removed key is treated as the loosest possible move.
check_ceiling() {
    local table="$1" key="$2" prev now
    prev=$(table_int "$table" "$key" "$prev_toml") || return 0
    now=$(table_int "$table" "$key" "$quality_toml") || {
        echo "quality.toml: [${table}].${key} removed (was ${prev}) - deleting a ceiling disables its gate, the loosest possible move" >>"$fail_marker"
        return 0
    }
    if [ "$now" -gt "$prev" ]; then
        echo "quality.toml: [${table}].${key} ceiling increased (${prev} -> ${now}) - ceilings may only fall" >>"$fail_marker"
    fi
}

# Every `"path" = N` line under `[table]`, as "path N" rows with the TOML quoting stripped.
exceptions_of() {
    awk -v want="[$1]" '
        /^\[/ { in_table = ($0 == want); next }
        !in_table { next }
        /^[[:space:]]*#/ || /^[[:space:]]*$/ { next }
        {
            n = split($0, parts, "\"")
            if (n < 3) next
            key = parts[2]
            rest = parts[3]
            sub(/^[[:space:]]*=[[:space:]]*/, "", rest)
            gsub(/[[:space:]]+$/, "", rest)
            if (rest ~ /^[0-9]+$/) print key, rest
        }
    ' "$2"
}

# An exception entry may only shrink; a brand-new entry always fails outright (this tree
# never gets to add debt through the ratchet - see the file header).
check_exceptions() {
    local table="$1" prev_exceptions now_exceptions
    prev_exceptions=$(exceptions_of "$table" "$prev_toml")
    now_exceptions=$(exceptions_of "$table" "$quality_toml")
    [ -z "$now_exceptions" ] && return 0
    printf '%s\n' "$now_exceptions" | while IFS=' ' read -r path ceiling; do
        [ -z "$path" ] && continue
        prev_ceiling=$(printf '%s\n' "$prev_exceptions" | awk -v p="$path" '$1 == p { print $2; found = 1 } END { if (!found) exit 1 }') || prev_ceiling=""
        if [ -z "$prev_ceiling" ]; then
            echo "quality.toml: [${table}] gained a new entry '${path}' - exceptions are debt with a ceiling, never permission; this repo never adds one through the ratchet, split the file instead" >>"$fail_marker"
        elif [ "$ceiling" -gt "$prev_ceiling" ]; then
            echo "quality.toml: [${table}] '${path}' raised from ${prev_ceiling} to ${ceiling} - exception ceilings may only fall" >>"$fail_marker"
        fi
    done
}

check_ceiling file_size all
check_exceptions file_size.exceptions

if [ -s "$fail_marker" ]; then
    sort "$fail_marker"
    echo "" >&2
    echo "ratchet guard FAILED - see violations above." >&2
    exit 1
fi
echo "ratchet: OK"
