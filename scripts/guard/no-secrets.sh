#!/usr/bin/env bash
# FilePath: scripts/guard/no-secrets.sh
# Never commit a secret. Two layers:
#
#   1. A built-in scan for Groq API keys (`gsk_` followed by 20+ key characters), the one
#      credential every user of this app pastes in and the most likely to land in a fixture or
#      a copied config. It needs no tooling, so it runs everywhere.
#   2. gitleaks with .gitleaks.toml (the full default rule set plus the same Groq rule), when
#      installed. CI installs it, so a machine without gitleaks is still covered on push.
#
# Scope, checked in this order:
#   - SV_GUARD_HOOK=1 (pre-commit, via run-all.sh --staged): only what is staged, so a commit
#     is gated in well under a second.
#   - SV_GUARD_RANGE set (pre-push): gitleaks scans only that `git log` range, the commits
#     actually being pushed; the Groq scan covers the tracked tree.
#   - Otherwise (`just guard`, CI): the tracked tree and the whole repository history.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

groq_pattern='gsk_[A-Za-z0-9]{20,}'
fail=0

# --- 1: Groq keys ----------------------------------------------------------------------------
if [ "${SV_GUARD_HOOK:-0}" = "1" ]; then
    hits=$(git diff --cached -U0 --no-color --diff-filter=ACM |
        grep -E '^\+' | grep -vE '^\+\+\+ ' | grep -cE "$groq_pattern" || true)
    if [ "$hits" != "0" ]; then
        echo "no-secrets: ${hits} staged line(s) contain a Groq API key (gsk_...) - remove it" >&2
        git diff --cached --name-only --diff-filter=ACM -z |
            xargs -0 grep -lE "$groq_pattern" -- 2>/dev/null >&2 || true
        fail=1
    fi
else
    status=0
    found=$(git grep -lIE "$groq_pattern" 2>/dev/null) || status=$?
    if [ "$status" -ge 2 ]; then
        echo "no-secrets: internal error - git grep failed (exit ${status})" >&2
        exit 1
    fi
    if [ -n "$found" ]; then
        while IFS= read -r file; do
            echo "${file}: contains a Groq API key (gsk_...) - remove it and rotate the key" >&2
        done <<<"$found"
        fail=1
    fi
fi

# --- 2: gitleaks ------------------------------------------------------------------------------
if command -v gitleaks >/dev/null 2>&1; then
    if [ "${SV_GUARD_HOOK:-0}" = "1" ]; then
        echo "no-secrets: scanning staged changes with gitleaks"
        gitleaks git --staged --no-banner --redact --log-level warn . || fail=1
    elif ! git rev-parse --verify --quiet HEAD >/dev/null; then
        echo "no-secrets: no commits yet - nothing in history for gitleaks to scan"
    elif [ -n "${SV_GUARD_RANGE:-}" ]; then
        echo "no-secrets: scanning revision range ${SV_GUARD_RANGE} with gitleaks"
        gitleaks git --no-banner --redact --log-level warn --log-opts="${SV_GUARD_RANGE}" . ||
            fail=1
    else
        echo "no-secrets: scanning the whole repository history with gitleaks"
        gitleaks git --no-banner --redact --log-level warn . || fail=1
    fi
else
    echo "no-secrets: gitleaks is not installed - only the Groq key scan ran" \
        "(install it: brew install gitleaks)" >&2
fi

if [ "$fail" -ne 0 ]; then
    echo "no-secrets: FAILED - remove the flagged value; if it is provably not a credential," \
        "add its fingerprint to .gitleaksignore with a justification." >&2
    exit 1
fi

echo "no-secrets: OK"
