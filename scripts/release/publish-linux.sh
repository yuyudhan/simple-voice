#!/usr/bin/env bash
# FilePath: scripts/release/publish-linux.sh
# Adds the Linux release built by scripts/release/build-linux.sh (the per-user tarball that
# install-linux.sh installs, the optional .deb, and their checksums) to the GitHub release
# vVERSION that scripts/release/publish.sh created, then rewrites the release's SHA256SUMS
# (scripts/release/checksums.sh). Runs on the publishing Mac, which holds the GitHub
# credentials; the Linux build machine needs none. It runs on Linux too, given `gh`.
#
# Usage: scripts/release/publish-linux.sh VERSION
#   Reads Simple-Voice_VERSION_amd64.tar.gz, Simple-Voice_VERSION_amd64.deb and their .sha256
#   from $CARGO_TARGET_DIR/release-assets/VERSION/ (default target dir: <repo>/target).
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
files=()
for asset in "Simple-Voice_${version}_amd64.tar.gz" "Simple-Voice_${version}_amd64.deb"; do
    for file in "$asset" "${asset}.sha256"; do
        [ -f "${assets}/${file}" ] || {
            echo "publish-linux: expected ${assets}/${file}; copy it from the Linux build" >&2
            exit 1
        }
        files+=("${assets}/${file}")
    done
    (cd "$assets" && sha256sum -c "${asset}.sha256" >/dev/null 2>&1 ||
        shasum -a 256 -c "${asset}.sha256" >/dev/null) || {
        echo "publish-linux: ${asset} does not match its .sha256" >&2
        exit 1
    }
done
gh release view "$tag" >/dev/null 2>&1 || {
    echo "publish-linux: release ${tag} does not exist; publish the macOS build first" >&2
    exit 1
}
gh release upload "$tag" "${files[@]}" --clobber
echo "publish-linux: uploaded the Linux tarball and .deb to ${GH_REPO} ${tag}"
"${root}/scripts/release/checksums.sh" "$version"
