# FilePath: justfiles/quality.just
# Formatting, linting, tests, hygiene guards and the aggregate gate. `check` is what pre-push
# and CI run; the narrower recipes are what you reach for while iterating. Rust compiles with
# SQLX_OFFLINE=true so no database is needed outside the sqlx recipes.

# Format everything in place: Rust, TOML, TypeScript/CSS, and Swift when swift-format exists.
[group('quality')]
fmt:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo fmt --all
    taplo fmt
    bun run format
    if command -v swift-format >/dev/null 2>&1; then
        swift-format format --in-place --recursive --parallel engine/Package.swift engine/Sources
    elif swift format --version >/dev/null 2>&1; then
        swift format format --in-place --recursive --parallel engine/Package.swift engine/Sources
    else
        echo "fmt: swift-format not found - engine/ left as is"
    fi

# Check formatting without writing anything.
[group('quality')]
fmt-check:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/status.sh
    sv_step 1 4 "cargo fmt --check"
    cargo fmt --all --check
    sv_step 2 4 "taplo fmt --check"
    taplo fmt --check
    sv_step 3 4 "prettier --check"
    bun run format:check
    sv_step 4 4 "swift-format lint"
    if command -v swift-format >/dev/null 2>&1; then
        swift-format lint --strict --recursive --parallel engine/Package.swift engine/Sources
    elif swift format --version >/dev/null 2>&1; then
        swift format lint --strict --recursive --parallel engine/Package.swift engine/Sources
    else
        echo "fmt-check: swift-format not found - engine/ not checked"
    fi
    sv_ok "fmt-check"

# Lint everything: clippy with warnings as errors, eslint, tsc, typos, shellcheck.
[group('quality')]
lint: engine-present
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/status.sh
    start=$(date +%s)
    sv_step 1 5 "cargo clippy --workspace --all-targets -D warnings"
    SQLX_OFFLINE=true cargo clippy --workspace --all-targets -- -D warnings
    sv_step 2 5 "eslint"
    bun run lint
    sv_step 3 5 "tsc --noEmit"
    bun run typecheck
    sv_step 4 5 "typos"
    typos
    sv_step 5 5 "shellcheck"
    git ls-files -z -- '*.sh' '.githooks/*' | xargs -0 shellcheck -x
    sv_ok "lint ($(sv_duration "$start"))"

# Run the Rust test suite (nextest when installed) and compile the Swift engine helper.
[group('quality')]
test: engine-present
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/status.sh
    start=$(date +%s)
    sv_step 1 2 "rust tests"
    if cargo nextest --version >/dev/null 2>&1; then
        SQLX_OFFLINE=true cargo nextest run --workspace --no-tests=pass
    else
        SQLX_OFFLINE=true cargo test --workspace
    fi
    sv_step 2 2 "swift build (engine)"
    swift build --package-path engine
    sv_ok "test ($(sv_duration "$start"))"

# Run every hygiene guard via scripts/guard/run-all.sh, or one by name (e.g. just guard no-todo).
[group('quality')]
guard NAME='all':
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "{{NAME}}" = "all" ]; then
        exec scripts/guard/run-all.sh
    fi
    script="scripts/guard/{{NAME}}.sh"
    if [ -x "$script" ]; then
        exec "$script"
    fi
    echo "no such guard: {{NAME}}" >&2
    echo "available: $(for f in scripts/guard/*.sh; do basename "$f" .sh; done | grep -v '^run-all$' | tr '\n' ' ')" >&2
    exit 1

# The full gate, run serially: fmt-check, guard, lint, test, sqlx-check. Pre-push and CI run it.
[group('quality')]
check:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/lib/status.sh
    run_start=$(date +%s)
    table=""
    run_gate() {
        local n="$1" label="$2" start status dur
        sv_phase "[$n/5] $label"
        start=$(date +%s)
        status=0
        just "$label" || status=$?
        dur=$(sv_duration "$start")
        if [ "$status" -ne 0 ]; then
            sv_fail "$label ($dur)"
            echo "check: FAILED at '$label' - see output above." >&2
            exit "$status"
        fi
        table="${table}$(printf '%-14s %8s  ok' "$label" "$dur")"$'\n'
    }
    run_gate 1 fmt-check
    run_gate 2 guard
    run_gate 3 lint
    run_gate 4 test
    run_gate 5 sqlx-check
    echo ""
    printf '%-14s %8s  %s\n' "GATE" "TIME" "STATUS"
    printf '%s' "$table"
    sv_ok "check ($(sv_duration "$run_start"))"
