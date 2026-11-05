#!/usr/bin/env bash
# FilePath: engine-linux/check.sh
# Talks to a built Linux engine helper over its real stdin/stdout protocol: `ping` must answer
# with the helper's version and `model_status` with Whisper Turbo's status, and the helper must
# exit once stdin closes. `just test` runs it after the unit tests.
#
# Usage: engine-linux/check.sh PATH-TO-simple-voice-engine
set -euo pipefail

engine="${1:?usage: $0 PATH-TO-simple-voice-engine}"
models=$(mktemp -d)
trap 'rm -rf "$models"' EXIT

replies=$(printf '%s\n' \
    '{"id":1,"cmd":"ping"}' \
    '{"id":2,"cmd":"model_status","model":"whisper-large-v3-turbo"}' \
    '{"id":3,"cmd":"model_status","model":"apple-intelligence"}' |
    timeout 20 "$engine" --models-dir "$models" 2>/dev/null)

expect() {
    if ! grep -q -- "$1" <<<"$replies"; then
        echo "engine check: no reply matching $1 in:" >&2
        echo "$replies" >&2
        exit 1
    fi
}
expect '"id":1,"ok":true,"result":{"version":"'
expect '"id":2,"ok":true,"result":{.*"status":"not_downloaded"'
expect '"id":3,"ok":true,"result":{.*"status":"unsupported"'
echo "engine check: OK ($(grep -c '"ok":true' <<<"$replies") replies)"
