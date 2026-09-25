#!/usr/bin/env bash
# FilePath: scripts/release/bump-version.sh
# Sets one version everywhere it is declared - package.json, src-tauri/tauri.conf.json and the
# root Cargo.toml [workspace.package] - refreshes the workspace entries in Cargo.lock, commits,
# and creates the annotated tag vVERSION. Pushing the tag is left to the caller because it
# starts the public release workflow.
#
# Usage: scripts/release/bump-version.sh 0.2.0
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

version="${1:-}"
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "usage: $0 MAJOR.MINOR.PATCH (got '${version}')" >&2
    exit 1
fi
tag="v${version}"

if [ -n "$(git status --porcelain)" ]; then
    echo "release: the working tree has uncommitted changes - commit or stash them first" >&2
    exit 1
fi
if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
    echo "release: tag ${tag} already exists" >&2
    exit 1
fi

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

git add package.json src-tauri/tauri.conf.json Cargo.toml Cargo.lock
git commit -m "🔖 Release ${tag}"
git tag -a "$tag" -m "Simple Voice ${tag}"

echo ""
echo "Tagged ${tag}. Publish it with:"
echo "    git push origin HEAD ${tag}"
