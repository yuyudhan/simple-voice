#!/usr/bin/env bash
# FilePath: scripts/guard/run-all.sh
# Runs every hygiene guard in this directory (scripts/guard/*.sh, except
# itself) against the git-tracked tree, numbering each one (`--- [3/12]
# file-size ---`), reporting elapsed wall time per guard, continuing past
# a failure so one run surfaces every violation in a single pass, and
# exiting non-zero with a tally if any guard failed. `just guard` calls
# this runner; pre-commit calls it with --staged and pre-push reaches it through `just check`.
#
# A guard script that is missing, unreadable, or not executable is a HARD FAIL here with an
# explicit message -- never a bare "Permission denied" shell error left to speak for itself.
# Once a script IS runnable, a shell syntax error inside it is also a FAILURE, not a skip:
# each guard is invoked as "$script" so it runs through its own shebang, and a syntax-error
# exit is a non-zero status like any other -- there is no separate "couldn't run it" path
# that would let a broken guard silently drop out of the tally. Guards may be #!/usr/bin/env bash or
# #!/bin/bash; this runner does not care, it only needs the kernel to be able to execute the
# file once the permission checks above pass.
#
# --staged exports SV_GUARD_HOOK=1 for guards supporting staged-only checks, such as
# no-secrets.sh (gitleaks on the index instead of the whole history). It does not filter file
# lists itself: guards that ignore this variable still scan the full tracked tree, which is
# fast enough for every commit.
set -euo pipefail

self="scripts/guard/run-all.sh"
root="$(cd "$(dirname "$0")/../.." && pwd)"

case "${1:-}" in
    --staged)
        SV_GUARD_HOOK=1
        export SV_GUARD_HOOK
        ;;
    "") ;;
    *)
        echo "$self: unknown argument '$1' (expected --staged or nothing)" >&2
        exit 1
        ;;
esac

cd "$root"

# Build the guard list first so the total is known before the loop starts numbering.
guards=()
for script in scripts/guard/*.sh; do
    [ "$(basename "$script")" = "run-all.sh" ] && continue
    guards+=("$script")
done
total=${#guards[@]}

fail=0
passed=0
failed=0
i=0
run_start=$(date +%s)
for script in "${guards[@]}"; do
    i=$((i + 1))
    name=$(basename "$script" .sh)
    echo "--- [$i/$total] $name ---"
    guard_start=$(date +%s)
    if [ ! -e "$script" ]; then
        echo "$name: HARD FAIL -- $script does not exist" >&2
        status=1
    elif [ ! -r "$script" ]; then
        echo "$name: HARD FAIL -- $script is not readable" >&2
        status=1
    elif [ ! -x "$script" ]; then
        echo "$name: HARD FAIL -- $script is not executable (chmod +x it)" >&2
        status=1
    else
        status=0
        "$script" || status=$?
    fi
    guard_end=$(date +%s)
    echo "($name: $((guard_end - guard_start))s)"
    if [ "$status" -eq 0 ]; then
        passed=$((passed + 1))
    else
        fail=1
        failed=$((failed + 1))
    fi
done
run_end=$(date +%s)

echo ""
echo "guard: $passed passed, $failed failed (total $((run_end - run_start))s)"
if [ "$fail" -ne 0 ]; then
    echo "one or more guards failed -- see output above." >&2
    exit 1
fi
echo "all guards passed."
