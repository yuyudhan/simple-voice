#!/usr/bin/env bash
# FilePath: scripts/release/build.sh
# Builds the release app in the current directory and packs it as the zip that scripts/install.sh
# downloads. The zip is made with `ditto`, which keeps the bundle's code signature and extended
# attributes intact, unlike plain `zip`.
#
# Usage: scripts/release/build.sh VERSION
#   The build log goes to stderr.
#   The zip lands in $CARGO_TARGET_DIR/release-assets/VERSION/ (default target dir: ./target).
#   Signing: never ad hoc, because an ad-hoc signature changes with every build and macOS then
#   forgets the user's permission grants on upgrade. APPLE_SIGNING_IDENTITY names the certificate
#   (default: the self-signed "Simple Voice Release" from scripts/release/signing-identity.sh);
#   the build is notarized only when APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID are set too.
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

identity="${APPLE_SIGNING_IDENTITY:-Simple Voice Release}"
if [ "$identity" = "-" ]; then
    echo "build: releases are never ad-hoc signed; unset APPLE_SIGNING_IDENTITY" >&2
    exit 1
fi
# Without -v: the self-signed certificate is untrusted, which codesign accepts.
if ! security find-identity -p codesigning | grep -Fq "\"${identity}\""; then
    echo "build: no \"${identity}\" signing identity in the keychain;" \
        "run scripts/release/signing-identity.sh create or restore" >&2
    exit 1
fi
export APPLE_SIGNING_IDENTITY="$identity"
# Tauri notarizes when all three are set; a partial set is a mistake, not a choice.
if [ -n "${APPLE_ID:-}${APPLE_PASSWORD:-}${APPLE_TEAM_ID:-}" ] &&
    { [ -z "${APPLE_ID:-}" ] || [ -z "${APPLE_PASSWORD:-}" ] || [ -z "${APPLE_TEAM_ID:-}" ]; }; then
    echo "build: APPLE_ID, APPLE_PASSWORD and APPLE_TEAM_ID are all needed to notarize" >&2
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
# A designated requirement naming a hash would tie grants to this one build.
requirement=$(codesign -d -r- "$app" 2>&1 | sed -n 's/^designated => //p')
case "$requirement" in
*certificate*) ;;
*)
    echo "build: the bundle's designated requirement names no certificate: ${requirement}" >&2
    exit 1
    ;;
esac
assets="${target_dir}/release-assets/${version}"
rm -rf "$assets"
mkdir -p "$assets"
ditto -c -k --keepParent "$app" "${assets}/Simple-Voice_${version}_aarch64.zip"
echo "build: packed ${assets}/Simple-Voice_${version}_aarch64.zip" >&2
