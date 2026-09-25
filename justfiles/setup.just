# FilePath: justfiles/setup.just
# One-shot bootstrap for a fresh clone and a toolchain report.

# First-time setup: JavaScript dependencies, git hooks, then a toolchain report.
[group('setup')]
setup:
    bun install
    just hooks
    just doctor

# Report required and optional tools. Fails when a required one is missing.
[group('setup')]
doctor:
    #!/usr/bin/env bash
    set -euo pipefail
    missing=0
    require() {
        local tool="$1" hint="$2"
        if command -v "$tool" >/dev/null 2>&1; then
            printf 'OK       %-14s %s\n' "$tool" "$(command -v "$tool")"
        else
            printf 'MISSING  %-14s install: %s\n' "$tool" "$hint"
            missing=$((missing + 1))
        fi
    }
    optional() {
        local tool="$1" hint="$2"
        if command -v "$tool" >/dev/null 2>&1; then
            printf 'OK       %-14s %s\n' "$tool" "$(command -v "$tool")"
        else
            printf 'optional %-14s install: %s\n' "$tool" "$hint"
        fi
    }
    echo "Required"
    require rustup "https://rustup.rs"
    require cargo "https://rustup.rs"
    require bun "brew install oven-sh/bun/bun"
    require just "brew install just"
    require swift "Xcode 16 or newer (xcode-select --install is not enough; install Xcode)"
    require xcodebuild "Xcode 16 or newer from the App Store"
    require sqlx "cargo install sqlx-cli --no-default-features --features sqlite,rustls --locked"
    require taplo "brew install taplo"
    require typos "brew install typos-cli"
    require shellcheck "brew install shellcheck"
    echo ""
    echo "Optional"
    optional gitleaks "brew install gitleaks (full secrets scan; the Groq key scan runs without it)"
    optional cargo-nextest "cargo install cargo-nextest (faster tests; cargo test is the fallback)"
    optional swift-format "ships with Xcode 16 as 'swift format'; formats engine/"
    echo ""
    hooks_path=$(git config core.hooksPath || true)
    if [ "$hooks_path" = ".githooks" ]; then
        echo "OK       git hooks      core.hooksPath = .githooks"
    else
        echo "MISSING  git hooks      run: just hooks"
        missing=$((missing + 1))
    fi
    if command -v xcodebuild >/dev/null 2>&1; then
        echo ""
        xcodebuild -version | sed -n 1p
    fi
    if [ "$missing" -gt 0 ]; then
        echo ""
        echo "${missing} required item(s) missing - install them, then run: just doctor" >&2
        exit 1
    fi
    echo ""
    echo "Toolchain ready. Next: just dev"
