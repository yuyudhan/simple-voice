#!/usr/bin/env bash
# FilePath: scripts/release/publish-tag.sh
# Builds an already pushed tag vVERSION on this Mac and publishes it: for a release whose publish
# step failed after the push, or a tag that was never published. The app is built from a clean,
# detached worktree of the tag, so uncommitted or later work never ships; the release scripts
# come from this checkout. Cargo reuses this checkout's target/ directory.
#
# Usage: scripts/release/publish-tag.sh VERSION
#   Needs `gh` logged in with push access to yuyudhan/simple-voice. Signing: see build.sh.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH (got '${version}')" >&2
    exit 1
fi
tag="v${version}"

gh auth status >/dev/null 2>&1 || {
    echo "publish-tag: log in with \`gh auth login\` first" >&2
    exit 1
}
local_commit=$(git rev-parse -q --verify "refs/tags/${tag}^{commit}") || {
    echo "publish-tag: tag ${tag} does not exist; \`just release ${version}\` creates it" >&2
    exit 1
}
remote_commit=$(git ls-remote origin "refs/tags/${tag}^{}" | cut -f1)
if [ "$remote_commit" != "$local_commit" ]; then
    echo "publish-tag: push the tag first: git push origin ${tag}" >&2
    exit 1
fi

build_dir=$(mktemp -d)
trap 'git -C "$root" worktree remove --force "${build_dir}/src" >/dev/null 2>&1 || true
rm -rf "$build_dir"' EXIT
git worktree add --detach "${build_dir}/src" "$tag"

export CARGO_TARGET_DIR="${root}/target"
(cd "${build_dir}/src" && "${root}/scripts/release/build.sh" "$version")
scripts/release/publish.sh "$version"
echo "publish-tag: ${tag} is live: curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash"
