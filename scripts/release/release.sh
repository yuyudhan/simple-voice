#!/usr/bin/env bash
# FilePath: scripts/release/release.sh
# Cuts a release entirely on this Mac: sets VERSION in package.json, src-tauri/tauri.conf.json
# and the root Cargo.toml [workspace.package], checks the cask, builds and packs the app, and
# only when that succeeds commits `🔖 Release vVERSION`, tags it, pushes branch and tag (the
# pre-push hook runs `just check`), creates the GitHub release and publishes the cask to the tap.
# A failure before the commit restores the version files, so nothing half-done is left behind.
#
# Usage: scripts/release/release.sh 0.2.0
#   Needs a clean working tree on a branch that is not behind origin, and `gh` logged in with
#   push access to yuyudhan/simple-voice and yuyudhan/homebrew-tap. Signing: see build.sh.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH (got '${version}')" >&2
    exit 1
fi
tag="v${version}"
files=(package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock)

if [ -n "$(git status --porcelain)" ]; then
    echo "release: the working tree has uncommitted changes - commit or stash them first" >&2
    exit 1
fi
branch=$(git symbolic-ref --short -q HEAD) || {
    echo "release: check out a branch first" >&2
    exit 1
}
gh auth status >/dev/null 2>&1 || {
    echo "release: log in with \`gh auth login\` first" >&2
    exit 1
}
git fetch --quiet --tags origin
if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo "release: tag ${tag} already exists; \`just publish ${version}\` republishes it" >&2
    exit 1
fi
if git rev-parse -q --verify "refs/remotes/origin/${branch}" >/dev/null &&
    ! git merge-base --is-ancestor "origin/${branch}" HEAD; then
    echo "release: ${branch} is behind origin/${branch}; pull first" >&2
    exit 1
fi

committed=false
restore() {
    if [ "$committed" = false ]; then
        git checkout --quiet -- "${files[@]}"
        echo "release: failed before the release commit; version files restored" >&2
    fi
}
trap restore EXIT

# The first "version" key in each JSON file is the top-level one.
for file in package.json src-tauri/tauri.conf.json; do
    VERSION="$version" perl -0pi -e 's/"version"(\s*):(\s*)"[^"]*"/"version"$1:$2"$ENV{VERSION}"/' "$file"
done
# Only the version line inside [workspace.package]; dependency versions stay untouched.
VERSION="$version" perl -0pi -e \
    's/(\[workspace\.package\][^\[]*?\nversion\s*=\s*)"[^"]*"/$1"$ENV{VERSION}"/' Cargo.toml
for file in package.json src-tauri/tauri.conf.json Cargo.toml; do
    if ! grep -q "\"${version}\"" "$file"; then
        echo "release: failed to set version ${version} in ${file}" >&2
        exit 1
    fi
done
cargo update --workspace

scripts/release/check-cask.sh "$version"
signed=$(scripts/release/build.sh "$version")

git add "${files[@]}"
git commit -m "🔖 Release ${tag}"
git tag -a "$tag" -m "Simple Voice ${tag}"
committed=true

if ! git push origin HEAD "$tag"; then
    echo "release: push failed; fix it, then: git push origin HEAD ${tag} && just publish ${version}" >&2
    exit 1
fi
scripts/release/publish.sh "$version" "$signed"
echo "release: ${tag} is live: brew install --cask yuyudhan/tap/simple-voice"
