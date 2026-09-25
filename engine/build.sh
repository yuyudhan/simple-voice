#!/usr/bin/env bash
# FilePath: engine/build.sh
# Builds the Swift engine helper and installs it where Tauri expects the sidecar:
# src-tauri/binaries/simple-voice-engine-<target-triple> (bundle.externalBin naming rule).
#
# Usage: engine/build.sh [debug|release]   (default: debug)
set -euo pipefail

configuration="${1:-debug}"
case "$configuration" in
    debug | release) ;;
    *)
        echo "usage: $0 [debug|release]" >&2
        exit 2
        ;;
esac

engine_dir="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$engine_dir/.." && pwd)"
binaries="$root/src-tauri/binaries"
target="$binaries/simple-voice-engine-aarch64-apple-darwin"

swift build --package-path "$engine_dir" -c "$configuration" --arch arm64 --product simple-voice-engine
bin_dir="$(swift build --package-path "$engine_dir" -c "$configuration" --arch arm64 --show-bin-path)"

mkdir -p "$binaries"
cp "$bin_dir/simple-voice-engine" "$target"
chmod 755 "$target"
echo "installed $target ($configuration)"
