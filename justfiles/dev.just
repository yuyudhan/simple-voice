# FilePath: justfiles/dev.just
# Running and building the app. Tauri refuses to compile without the sidecar binary it bundles,
# so every recipe that compiles src-tauri makes sure the engine helper exists first.

# Run the app with hot reload (builds the debug engine helper first).
[group('develop')]
dev: (engine "debug")
    bun run tauri dev

# Build the release app bundle and DMG for Apple Silicon.
[group('develop')]
build: (engine "release")
    bun run tauri build --target aarch64-apple-darwin

# Build the Swift engine helper into src-tauri/binaries/. MODE: debug (default) | release
[group('develop')]
engine MODE='debug':
    bash engine/build.sh {{MODE}}

# Build the debug engine helper only when it is missing, so cargo-based gates can compile.
[private]
engine-present:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -x "{{engine_bin}}" ]; then
        echo "engine helper missing at {{engine_bin}} - building it (debug)"
        bash engine/build.sh debug
    fi
