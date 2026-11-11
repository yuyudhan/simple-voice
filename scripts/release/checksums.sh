#!/usr/bin/env bash
# FilePath: scripts/release/checksums.sh
# Writes the SHA256SUMS of every asset on the GitHub release vVERSION and uploads it to that
# release, replacing the previous one. The sums are the SHA-256 digests GitHub reports for the
# uploaded files, so they describe exactly what the release serves. install.sh checks
# install-linux.sh against this file, and yuyudhan.github.io's sync-installers workflow checks
# install.sh against it before it publishes https://yuyudhan.github.io/simple-voice/install.
#
# publish.sh and publish-linux.sh run it after every upload. After uploading a changed installer
# to an existing release (docs/internal/releasing.md#the-install-script), run it by hand.
#
# Usage: scripts/release/checksums.sh VERSION
#   `gh` must be logged in with push access; GH_REPO overrides yuyudhan/simple-voice.
set -euo pipefail

if [ "$#" -ne 1 ] || ! [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH" >&2
    exit 1
fi
version="$1"
tag="v${version}"
export GH_REPO="${GH_REPO:-yuyudhan/simple-voice}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

gh release view "$tag" --json assets \
    --jq '.assets[] | select(.name != "SHA256SUMS") | [.name, (.digest // "")] | @tsv' >"${work}/assets.tsv"
[ -s "${work}/assets.tsv" ] || {
    echo "checksums: release ${tag} has no assets" >&2
    exit 1
}
while IFS=$'\t' read -r name digest; do
    case "$digest" in
        sha256:[0-9a-f]*) printf '%s  %s\n' "${digest#sha256:}" "$name" ;;
        *)
            echo "checksums: GitHub reports no SHA-256 digest for ${name} on ${tag}" >&2
            exit 1
            ;;
    esac
done <"${work}/assets.tsv" | LC_ALL=C sort -k 2 >"${work}/SHA256SUMS"
[ "$(wc -l <"${work}/SHA256SUMS")" -eq "$(wc -l <"${work}/assets.tsv")" ] || {
    echo "checksums: could not write a sum for every asset of ${tag}" >&2
    exit 1
}

gh release upload "$tag" "${work}/SHA256SUMS" --clobber
echo "checksums: uploaded SHA256SUMS ($(wc -l <"${work}/SHA256SUMS" | tr -d ' ') assets) to ${GH_REPO} ${tag}"
