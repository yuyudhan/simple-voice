#!/usr/bin/env bash
# FilePath: scripts/release/publish.sh
# Publishes a release built by scripts/release/build.sh: uploads the zip, its checksum,
# scripts/install.sh (served as releases/latest/download/install.sh, and from there at
# https://yuyudhan.github.io/simple-voice/install) and the Linux installer it hands over to,
# scripts/install-linux.sh, to the GitHub release of the pushed tag vVERSION, creating the
# release if needed, then the release's SHA256SUMS (scripts/release/checksums.sh). The Linux
# .deb is built on Linux and added afterwards by scripts/release/publish-linux.sh.
#
# Usage: scripts/release/publish.sh VERSION
#   The zip is read from $CARGO_TARGET_DIR/release-assets/VERSION/ (default target dir:
#   <repo>/target). `gh` must be logged in with push access; GH_REPO overrides
#   yuyudhan/simple-voice.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"

if [ "$#" -ne 1 ] || ! [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH" >&2
    exit 1
fi
version="$1"
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

installers=("${root}/scripts/install.sh" "${root}/scripts/install-linux.sh")
if gh release view "$tag" >/dev/null 2>&1; then
    gh release upload "$tag" "${assets}/${asset}" "${assets}/${asset}.sha256" "${installers[@]}" --clobber
else
    gh release create "$tag" "${assets}/${asset}" "${assets}/${asset}.sha256" "${installers[@]}" \
        --title "Simple Voice ${tag}" --generate-notes --verify-tag
fi
echo "publish: uploaded ${asset} (sha256 ${sha256}), install.sh and install-linux.sh to ${GH_REPO} ${tag}"
"${root}/scripts/release/checksums.sh" "$version"
