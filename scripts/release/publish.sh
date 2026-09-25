#!/usr/bin/env bash
# FilePath: scripts/release/publish.sh
# Publishes a release built by scripts/release/build.sh: uploads the zip and its checksum to the
# GitHub release of the pushed tag vVERSION (creating the release if needed), then pushes the
# rendered cask to the tap.
#
# Usage: scripts/release/publish.sh VERSION SIGNED
#   SIGNED is the value build.sh printed. The zip is read from
#   $CARGO_TARGET_DIR/release-assets/VERSION/ (default target dir: <repo>/target). `gh` must be
#   logged in with push access to both repositories; GH_REPO overrides yuyudhan/simple-voice.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"

if [ "$#" -ne 2 ] || ! [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || ! [[ "$2" =~ ^(true|false)$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH true|false" >&2
    exit 1
fi
version="$1"
signed="$2"
tag="v${version}"
export GH_REPO="${GH_REPO:-yuyudhan/simple-voice}"

assets="${CARGO_TARGET_DIR:-${root}/target}/release-assets/${version}"
asset="Simple-Voice_${version}_aarch64.zip"
if [ ! -f "${assets}/${asset}" ]; then
    echo "publish: expected ${assets}/${asset}; run scripts/release/build.sh first" >&2
    exit 1
fi
sha256=$(shasum -a 256 "${assets}/${asset}" | cut -d' ' -f1)
printf '%s  %s\n' "$sha256" "$asset" >"${assets}/${asset}.sha256"

if gh release view "$tag" >/dev/null 2>&1; then
    gh release upload "$tag" "${assets}/${asset}" "${assets}/${asset}.sha256" --clobber
else
    gh release create "$tag" "${assets}/${asset}" "${assets}/${asset}.sha256" \
        --title "Simple Voice ${tag}" --generate-notes --verify-tag
fi
echo "publish: uploaded ${asset} (sha256 ${sha256}) to ${GH_REPO} ${tag}"

"${root}/scripts/release/update-cask.sh" publish "$version" "$sha256" "$signed"
