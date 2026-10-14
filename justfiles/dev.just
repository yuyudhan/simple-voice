# FilePath: justfiles/dev.just
# Running and building the app. Tauri refuses to compile without the sidecar binary it bundles,
# so every recipe that compiles src-tauri makes sure the engine helper exists first.

# Run the app with hot reload (builds the debug engine helper first). Query macros check against
# a freshly migrated target/sqlx-dev.db, so a new or edited query compiles before `just
# sqlx-prepare` has refreshed the offline cache. tauri.dev.conf.json names the run "Simple Voice
# Dev" with its own bundle identifier, keeping it apart from the installed app.
[group('develop')]
dev: deps-present (engine "debug") sqlx-db
    DATABASE_URL={{sqlx_db_url}} bun run tauri dev --config src-tauri/tauri.dev.conf.json

# Build the release app bundle for Apple Silicon; Tauri's beforeBuildCommand builds the
# release engine helper first. Without a Developer ID the bundle is ad-hoc signed ("-"), which
# seals it and applies Entitlements.plist and the hardened runtime; an unsigned bundle would lack
# the microphone entitlement. Export APPLE_SIGNING_IDENTITY to use a real certificate instead.
[group('develop')]
build: deps-present
    APPLE_SIGNING_IDENTITY="${APPLE_SIGNING_IDENTITY:--}" bun run tauri build --target aarch64-apple-darwin

# Build the Swift engine helper into src-tauri/binaries/. MODE: debug (default) | release
[group('develop')]
engine MODE='debug':
    bash engine/build.sh {{MODE}}

# Build the debug engine helper only when it (or the llama.framework it links, which the app
# bundle embeds) is missing, so cargo-based gates can compile.
[private]
engine-present:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -x "{{engine_bin}}" ] || [ ! -d "src-tauri/binaries/llama.framework" ]; then
        echo "engine helper or llama.framework missing in src-tauri/binaries - building (debug)"
        bash engine/build.sh debug
    fi

# Install the JavaScript dependencies (prettier, eslint, tsc, vite, the Tauri CLI) when
# node_modules is missing or older than bun.lock, so a fresh clone or a cleaned tree can run
# every bun-backed recipe without a separate `just setup`. The lockfile is frozen: a recipe never
# rewrites bun.lock behind your back.
[private]
deps-present:
    #!/usr/bin/env bash
    set -euo pipefail
    marker=node_modules/.sv-deps-installed
    if [ ! -f "$marker" ] || [ bun.lock -nt "$marker" ]; then
        echo "JavaScript dependencies missing or out of date - running bun install"
        bun install --frozen-lockfile
        touch "$marker"
    fi
