#!/usr/bin/env bash
# FilePath: scripts/lib/status.sh
# shellcheck shell=bash
# Sourceable progress/status helpers for hooks and justfile recipes that can run silently for
# minutes (`just check`, a full-history gitleaks scan, `cargo test`) so a slow gate never
# looks hung to a developer or an agent watching the terminal.
#
# Usage:
#     source "scripts/lib/status.sh"
#     sv_phase "pre-push gate"
#     sv_step 1 3 "guards"
#     start=$(date +%s)
#     sv_heartbeat_start "just check"
#     just check || status=$?
#     sv_heartbeat_stop
#     sv_ok "just check ($(sv_duration "$start"))"
#
# Functions:
#   sv_phase TITLE               banner line opening a named phase
#   sv_step N TOTAL LABEL        numbered "[N/TOTAL] LABEL" line
#   sv_ok LABEL                  success line
#   sv_fail LABEL                failure line (stderr); does not exit
#   sv_duration START_EPOCH      format elapsed seconds since START_EPOCH as "9s" / "1m 04s"
#                                / "1h 02m"
#   sv_heartbeat_start LABEL     fork a background ticker that prints an elapsed-time line
#                                every 15s until stopped. A no-op when stdout is not a TTY,
#                                so it never spams a log file or a piped/redirected run.
#   sv_heartbeat_stop            kill the ticker started by sv_heartbeat_start, if any;
#                                safe to call even when no heartbeat is running.
#
# The heartbeat's child pid is tracked in SV_HEARTBEAT_PID and reaped by both
# sv_heartbeat_stop and an EXIT trap installed when the heartbeat starts, so a script that
# dies mid-gate (Ctrl-C, a `set -e` failure) never leaves a stray ticker process behind.

if [ -n "${SV_STATUS_SOURCED:-}" ]; then
    # shellcheck disable=SC2317  # unreachable only when this file is executed directly
    # rather than sourced; `|| true` covers that case's non-zero `return`.
    return 0 2>/dev/null || true
fi
SV_STATUS_SOURCED=1

SV_HEARTBEAT_PID=""

sv_phase() {
    printf '\n=== %s ===\n' "$1"
}

sv_step() {
    printf '[%s/%s] %s\n' "$1" "$2" "$3"
}

sv_ok() {
    printf 'OK: %s\n' "$1"
}

sv_fail() {
    printf 'FAILED: %s\n' "$1" >&2
}

sv_duration() {
    local start="$1" now elapsed
    now=$(date +%s)
    elapsed=$((now - start))
    if [ "$elapsed" -lt 60 ]; then
        printf '%ds' "$elapsed"
    elif [ "$elapsed" -lt 3600 ]; then
        printf '%dm %02ds' "$((elapsed / 60))" "$((elapsed % 60))"
    else
        printf '%dh %02dm' "$((elapsed / 3600))" "$(((elapsed % 3600) / 60))"
    fi
}

# Runs as the background ticker; never invoked directly.
_sv_heartbeat_loop() {
    local label="$1" start
    start=$(date +%s)
    while :; do
        sleep 15
        printf '... still running: %s (%s elapsed)\n' "$label" "$(sv_duration "$start")"
    done
}

sv_heartbeat_start() {
    local label="$1"
    SV_HEARTBEAT_PID=""
    [ -t 1 ] || return 0
    _sv_heartbeat_loop "$label" &
    SV_HEARTBEAT_PID=$!
    trap 'sv_heartbeat_stop' EXIT
}

sv_heartbeat_stop() {
    if [ -n "$SV_HEARTBEAT_PID" ]; then
        kill "$SV_HEARTBEAT_PID" 2>/dev/null || true
        wait "$SV_HEARTBEAT_PID" 2>/dev/null || true
        SV_HEARTBEAT_PID=""
    fi
    trap - EXIT
}
