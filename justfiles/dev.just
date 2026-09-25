# FilePath: justfiles/dev.just
# Running and building the app. Tauri refuses to compile without the sidecar binary it bundles,
# so every recipe that compiles src-tauri makes sure the engine helper exists first.

# Run the app with hot reload (builds the debug engine helper first). Query macros check against
# a freshly migrated target/sqlx-dev.db, so a new or edited query compiles before `just
# sqlx-prepare` has refreshed the offline cache.
[group('develop')]
dev: (engine "debug") sqlx-db
    DATABASE_URL={{sqlx_db_url}} bun run tauri dev

# Build the release app bundle and DMG for Apple Silicon. Without a Developer ID the bundle is
# ad-hoc signed ("-"), which seals it and applies Entitlements.plist and the hardened runtime;
# an unsigned bundle would lack the microphone entitlement. Export APPLE_SIGNING_IDENTITY to use
# a real certificate instead.
[group('develop')]
build: (engine "release")
    APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}" bun run tauri build --target aarch64-apple-darwin

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
