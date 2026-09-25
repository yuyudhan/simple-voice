# FilePath: justfiles/help.just
# Grouped command reference and git hook installation.

# Show all recipes grouped by workflow.
[group('help')]
help:
    @echo ""
    @echo "Simple Voice - Command Reference"
    @echo "================================"
    @echo ""
    @echo "SETUP"
    @echo "  just setup                bun install, git hooks, toolchain check"
    @echo "  just hooks                Point core.hooksPath at .githooks/"
    @echo "  just doctor               Report which required and optional tools are installed"
    @echo ""
    @echo "DEVELOP"
    @echo "  just dev                  Build the engine helper (debug) and run the app with hot reload"
    @echo "  just build                Release app bundle and DMG (builds the release engine helper)"
    @echo "  just engine [MODE]        Build the Swift engine helper: debug (default) | release"
    @echo ""
    @echo "QUALITY GATES"
    @echo "  just fmt                  Format Rust, TOML, TypeScript/CSS and Swift in place"
    @echo "  just fmt-check            Check formatting without writing"
    @echo "  just lint                 clippy (-D warnings), eslint, tsc, typos, shellcheck"
    @echo "  just test                 Rust tests (nextest when installed) + swift build of the engine"
    @echo "  just guard [NAME]         Every hygiene guard in scripts/guard/, or one by name"
    @echo "  just check                The full gate: fmt-check, guard, lint, test, sqlx-check"
    @echo ""
    @echo "SQLX (compile-time checked SQL)"
    @echo "  just sqlx-db              Recreate target/sqlx-dev.db and apply every migration"
    @echo "  just sqlx-prepare         Regenerate the committed .sqlx/ offline cache"
    @echo "  just sqlx-check           Fail when .sqlx/ does not match the queries in the code"
    @echo ""
    @echo "RELEASE"
    @echo "  just release VERSION      Bump every version field, commit, and tag vVERSION"
    @echo "  just cask-check           Run brew style and brew audit on the rendered Homebrew cask"
    @echo ""
    @echo "See README.md and docs/internal/."
    @echo ""

# Point core.hooksPath at .githooks/ so pre-commit, commit-msg and pre-push run automatically.
[group('setup')]
hooks:
    git config core.hooksPath .githooks
    chmod +x .githooks/* scripts/guard/*.sh scripts/release/*.sh
    @echo "core.hooksPath -> .githooks"
