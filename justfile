# FilePath: justfile
# Simple Voice task runner. Run `just` to list recipes, `just help` for the grouped reference.
# Recipes live in justfiles/ and are imported into this one namespace.

set shell := ["bash", "-euo", "pipefail", "-c"]

import "justfiles/help.just"
import "justfiles/setup.just"
import "justfiles/dev.just"
import "justfiles/quality.just"
import "justfiles/sqlx.just"
import "justfiles/release.just"

# The sqlx macros verify against this throwaway SQLite database (created by `just sqlx-db`);
# everything else compiles offline against the committed `.sqlx/` cache. The path is absolute
# because the macros resolve a relative SQLite path from each crate's own directory.
sqlx_db := justfile_directory() / "target/sqlx-dev.db"
sqlx_db_url := "sqlite:" + sqlx_db
migrations_dir := "crates/sv-storage/migrations"

# Tauri resolves the sidecar as binaries/<name>-<target triple>; engine/build.sh writes it here.
engine_bin := "src-tauri/binaries/simple-voice-engine-aarch64-apple-darwin"

# List every recipe.
[group('help')]
default:
    @just --list --unsorted
