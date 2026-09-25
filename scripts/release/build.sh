#!/usr/bin/env bash
# FilePath: scripts/release/build.sh
# Builds the release app in the current directory and packs it as the zip the cask downloads.
# The zip is made with `ditto`, which keeps the bundle's code signature and extended attributes
# intact, unlike plain `zip`; Homebrew unpacks it natively.
#
# Usage: scripts/release/build.sh VERSION
#   Prints `true` (Developer ID signed and notarized) or `false` as the last line of stdout; the
#   build log goes to stderr. The zip lands in $CARGO_TARGET_DIR/release-assets/VERSION/
#   (default target dir: ./target).
#   Signing: ad hoc unless APPLE_SIGNING_IDENTITY names a Developer ID in the keychain; the
#   build counts as notarized only when APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID are set too.
set -euo pipefail

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH (got '${version}')" >&2
    exit 1
fi

declared=$(sed -n 's/^    "version": "\([^"]*\)",$/\1/p' src-tauri/tauri.conf.json)
if [ "$declared" != "$version" ]; then
    echo "build: src-tauri/tauri.conf.json declares ${declared}, not ${version}" >&2
    exit 1
fi

if [ "${APPLE_SIGNING_IDENTITY:--}" = "-" ]; then
    export APPLE_SIGNING_IDENTITY="-"
    unset APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
    signed=false
elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_PASSWORD:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ]; then
    signed=true
else
    echo "build: APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID are needed to notarize" >&2
    exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$(pwd)/target}"
{
    bun install --frozen-lockfile
    # beforeBuildCommand in tauri.conf.json builds the release engine helper first.
    SQLX_OFFLINE=true bun run tauri build --target aarch64-apple-darwin --bundles app
} >&2

app="${target_dir}/aarch64-apple-darwin/release/bundle/macos/Simple Voice.app"
codesign --verify --deep --strict "$app" >&2
assets="${target_dir}/release-assets/${version}"
rm -rf "$assets"
mkdir -p "$assets"
ditto -c -k --keepParent "$app" "${assets}/Simple-Voice_${version}_aarch64.zip"
echo "build: packed ${assets}/Simple-Voice_${version}_aarch64.zip" >&2
echo "$signed"
