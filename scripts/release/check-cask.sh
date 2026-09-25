#!/usr/bin/env bash
# FilePath: scripts/release/check-cask.sh
# Renders the cask for both an ad-hoc signed and a notarized release and runs Homebrew's own
# `brew style` and `brew audit --strict` on each, so a cask that Homebrew would reject never
# reaches the tap. Homebrew only applies its cask rules to casks inside a tap, so each rendering
# is checked from a throwaway local tap that is removed again on exit.
#
# Usage: scripts/release/check-cask.sh [VERSION [SHA256]]
#   Defaults: the version in src-tauri/tauri.conf.json and a placeholder checksum.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
version="${1:-$(sed -n 's/^    "version": "\([^"]*\)",$/\1/p' "${root}/src-tauri/tauri.conf.json")}"
sha256="${2:-$(printf 'simple-voice' | shasum -a 256 | cut -d' ' -f1)}"

command -v brew >/dev/null || {
    echo "check-cask: Homebrew is required" >&2
    exit 1
}

tap="simple-voice/cask-check"
if brew tap | grep -qx "$tap"; then
    brew untap --force "$tap" >/dev/null
fi
brew tap-new --no-git "$tap" >/dev/null
trap 'brew untap --force "$tap" >/dev/null' EXIT
casks="$(brew --repository "$tap")/Casks"
mkdir -p "$casks"

for signed in false true; do
    echo "check-cask: signed=${signed}"
    "${root}/scripts/release/update-cask.sh" render "$version" "$sha256" "$signed" \
        >"${casks}/simple-voice.rb"
    brew style --cask "${tap}/simple-voice"
    brew audit --cask --strict "${tap}/simple-voice"
done
echo "check-cask: the cask passes brew style and brew audit"
