#!/usr/bin/env bash
# FilePath: scripts/lib/quality.sh
# shellcheck shell=bash
#
# Shared reader for quality.toml, the single source of truth for every mechanical size
# ceiling in this repo (see quality.toml's own header for what a ceiling and an exception
# entry each mean). Guards source this instead of hardcoding numbers or re-implementing a
# TOML reader; scripts/guard/ratchet.sh is the only thing that may ever change what a value
# here means over time.
#
# Usage (from a guard script under scripts/guard/):
#
#     root="$(cd "$(dirname "$0")/../.." && pwd)"
#     # shellcheck source=scripts/lib/quality.sh
#     source "${root}/scripts/lib/quality.sh"
#     limit=$(quality_limit file_size all) || exit 1
#     exception=$(quality_exception file_size.exceptions "$path")
#
# Contract:
#   quality_limit <section> <key>
#       Print the integer at quality.toml's [<section>].<key> on stdout. Hard-fails (message
#       on stderr, non-zero return) if quality.toml, the section, or the key is missing, or
#       the value is not a non-negative integer — a missing ceiling is a silently-disabled
#       gate, never a default.
#   quality_exception <table> <path>
#       Print the per-path ceiling recorded for "<path>" inside a quoted-key table such as
#       [file_size.exceptions]. Prints nothing (not an error, empty stdout, success return)
#       when <path> has no entry — callers decide what "no exception" means.
#
# Both functions resolve quality.toml from their own location (BASH_SOURCE), independent of
# the caller's cwd or $0, unless SV_QUALITY_TOML overrides the path (used by mutation
# proofs and tests to point at a scratch file without touching the real one).

if [ -n "${SV_QUALITY_SH_SOURCED:-}" ]; then
    # shellcheck disable=SC2317  # unreachable unless this file is somehow executed directly
    return 0 2>/dev/null || true
fi
SV_QUALITY_SH_SOURCED=1

_quality_toml_path() {
    local lib_dir root
    lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    root="$(cd "${lib_dir}/../.." && pwd)"
    printf '%s\n' "${SV_QUALITY_TOML:-${root}/quality.toml}"
}

quality_limit() {
    local section="$1" key="$2" quality_toml value
    quality_toml="$(_quality_toml_path)"
    if [ ! -f "$quality_toml" ]; then
        echo "quality_limit: missing ${quality_toml}" >&2
        return 1
    fi
    value=$(awk -v want="[$section]" -v key="$key" '
        /^\[/ { in_section = ($0 == want); next }
        in_section && $1 == key && $2 == "=" { print $3; found = 1 }
        END { if (!found) exit 1 }
    ' "$quality_toml") || {
        echo "quality_limit: quality.toml has no [${section}].${key} key" >&2
        return 1
    }
    case "$value" in
        '' | *[!0-9]*)
            echo "quality_limit: quality.toml [${section}].${key} is not a non-negative integer: '${value}'" >&2
            return 1
            ;;
    esac
    printf '%s\n' "$value"
}

quality_exception() {
    local table="$1" path="$2" quality_toml
    quality_toml="$(_quality_toml_path)"
    [ -f "$quality_toml" ] || return 0
    awk -v want="[$table]" -v want_path="$path" '
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
            if (key == want_path && rest ~ /^[0-9]+$/) print rest
        }
    ' "$quality_toml"
}
