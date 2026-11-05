#!/usr/bin/env bash
# FilePath: scripts/release/build-linux.sh
# Builds the Linux release of an already pushed tag vVERSION on an x86-64 Debian/Ubuntu machine:
# the .deb (`tauri build --bundles deb`; its beforeBuildCommand builds the release engine helper
# from engine-linux/), checked before it is called done: package name and version, the app's
# `--version`, and the bundled engine helper answering over its protocol. The .deb and its
# .sha256 land in target/release-assets/VERSION/ as Simple-Voice_VERSION_amd64.deb, the name
# install-linux.sh downloads. Copy both to the Mac that published the release and run
# scripts/release/publish-linux.sh there (docs/internal/releasing.md#linux).
#
# Usage: scripts/release/build-linux.sh VERSION
#   The checkout must be clean and at the tag, so only the released commit ships.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH (got '${version}')" >&2
    exit 1
fi
tag="v${version}"
if [ "$(uname -s)" != "Linux" ] || [ "$(uname -m)" != "x86_64" ]; then
    echo "build-linux: run this on x86-64 Linux" >&2
    exit 1
fi
if [ -n "$(git status --porcelain)" ]; then
    echo "build-linux: the working tree has uncommitted changes" >&2
    exit 1
fi
tag_commit=$(git rev-parse -q --verify "refs/tags/${tag}^{commit}") || {
    echo "build-linux: tag ${tag} is missing; git fetch --tags first" >&2
    exit 1
}
if [ "$(git rev-parse HEAD)" != "$tag_commit" ]; then
    echo "build-linux: check out ${tag} first (git checkout ${tag})" >&2
    exit 1
fi

bun install --frozen-lockfile
bun run tauri build --bundles deb

deb_dir="${CARGO_TARGET_DIR:-${root}/target}/release/bundle/deb"
built=$(find "$deb_dir" -maxdepth 1 -name "*_${version}_amd64.deb" -print -quit)
[ -n "$built" ] || {
    echo "build-linux: no ${version} .deb in ${deb_dir}" >&2
    exit 1
}
if [ "$(dpkg-deb -f "$built" Package)" != "simple-voice" ] ||
    [ "$(dpkg-deb -f "$built" Version)" != "$version" ]; then
    echo "build-linux: ${built} is not simple-voice ${version}" >&2
    exit 1
fi

check=$(mktemp -d)
trap 'rm -rf "$check"' EXIT
dpkg-deb -x "$built" "$check"
reported=$("${check}/usr/bin/simple-voice" --version)
[ "$reported" = "simple-voice ${version}" ] || {
    echo "build-linux: the packaged app reports '${reported}'" >&2
    exit 1
}
engine-linux/check.sh "${check}/usr/bin/simple-voice-engine"

assets="${CARGO_TARGET_DIR:-${root}/target}/release-assets/${version}"
asset="Simple-Voice_${version}_amd64.deb"
mkdir -p "$assets"
cp "$built" "${assets}/${asset}"
(cd "$assets" && sha256sum "$asset" >"${asset}.sha256")
echo "build-linux: ${assets}/${asset} ($(cut -d' ' -f1 "${assets}/${asset}.sha256"))"
