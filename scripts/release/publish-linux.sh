#!/usr/bin/env bash
# FilePath: scripts/release/publish-linux.sh
# Adds the Linux .deb built by scripts/release/build-linux.sh, and its checksum, to the GitHub
# release vVERSION that scripts/release/publish.sh created. Runs on the publishing Mac, which
# holds the GitHub credentials; the Linux build machine needs none.
#
# Usage: scripts/release/publish-linux.sh VERSION
#   Reads Simple-Voice_VERSION_amd64.deb and its .sha256 from
#   $CARGO_TARGET_DIR/release-assets/VERSION/ (default target dir: <repo>/target).
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
asset="Simple-Voice_${version}_amd64.deb"
for file in "$asset" "${asset}.sha256"; do
    [ -f "${assets}/${file}" ] || {
        echo "publish-linux: expected ${assets}/${file}; copy it from the Linux build" >&2
        exit 1
    }
done
(cd "$assets" && shasum -a 256 -c "${asset}.sha256" >/dev/null) || {
    echo "publish-linux: ${asset} does not match its .sha256" >&2
    exit 1
}
gh release view "$tag" >/dev/null 2>&1 || {
    echo "publish-linux: release ${tag} does not exist; publish the macOS build first" >&2
    exit 1
}
gh release upload "$tag" "${assets}/${asset}" "${assets}/${asset}.sha256" --clobber
echo "publish-linux: uploaded ${asset} to ${GH_REPO} ${tag}"
