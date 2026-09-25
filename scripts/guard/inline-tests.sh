#!/usr/bin/env bash
# FilePath: scripts/guard/inline-tests.sh
# Rust tests live in exactly one place: a single `#[cfg(test)] mod tests { ... }` block at the
# bottom of the file whose code they cover. This guard rejects every other layout so the tree
# never drifts back to three competing conventions:
#
#   - a tracked `*_tests.rs`, `*_tests_*.rs`, or `tests.rs` file anywhere under a crate `src/`;
#   - any tracked `.rs` file under a crate-root `tests/` directory (integration-test crates);
#   - a `#[cfg(test)]` attribute that is not immediately followed by `mod tests {` - that is
#     either a `#[path = "..."]`-redirected test module (tests living in another file), a
#     second test module in the same file, or a stray test-only item outside the block;
#   - more than one `#[cfg(test)]` attribute per file, or one that is not the file's last
#     module (production code after the test block hides from the size guards, which stop
#     counting at the first `#[cfg(test)]`).
#
# Keeping the block last and unique is what lets scripts/guard/file-size.sh count production
# lines by truncating at that attribute. Scoped to tracked `.rs` files under src-tauri/ and
# crates/. Operates on `git ls-files -z` output only and drops any path tracked but missing
# from the working tree before scanning.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

fail=0
files=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] || continue
    files+=("$f")
done < <(git ls-files -z -- 'src-tauri/*.rs' 'crates/*.rs')

if [ "${#files[@]}" -eq 0 ]; then
    echo "inline-tests: internal error - no tracked .rs files under src-tauri/ or crates/; this guard measured nothing" >&2
    exit 1
fi

for f in "${files[@]}"; do
    base=$(basename "$f")
    case "$f" in
        */tests/*)
            echo "${f}: integration-test directories are not allowed - move these tests inline into the module they cover as \`#[cfg(test)] mod tests { ... }\`"
            fail=1
            continue
            ;;
    esac
    case "$base" in
        tests.rs | *_tests.rs | *_tests_*.rs)
            echo "${f}: separate test files are not allowed - move these tests inline into the module they cover as \`#[cfg(test)] mod tests { ... }\`"
            fail=1
            continue
            ;;
    esac
done

violations=$(awk '
    FNR == 1 { count = 0; pending = 0; closed = 0; after = 0 }
    pending {
        pending = 0
        if ($0 !~ /^[[:space:]]*mod tests[[:space:]]*\{/) {
            printf "%s:%d: #[cfg(test)] must be immediately followed by `mod tests {` - no #[path]-redirected test modules, no test-only items outside the single test block\n", FILENAME, FNR
        }
        next
    }
    /^[[:space:]]*#\[cfg\(test\)\]/ {
        count++
        if (count > 1) {
            printf "%s:%d: second #[cfg(test)] in this file - keep exactly one `mod tests` block per file\n", FILENAME, FNR
        }
        pending = 1
        next
    }
    # The block ends at the first column-0 `}` after `mod tests {` (rustfmt puts every
    # top-level closer there). Anything but blank lines and comments after it is production
    # code hidden from the size guards.
    count == 1 && !closed && /^\}[[:space:]]*$/ { closed = 1; next }
    closed && !after && !/^[[:space:]]*$/ && !/^[[:space:]]*\/\// {
        after = 1
        printf "%s:%d: code after the #[cfg(test)] block - the test module must be the last item in the file\n", FILENAME, FNR
    }
' "${files[@]}") || {
    echo "inline-tests: internal error - awk failed scanning the candidate file list" >&2
    exit 1
}

if [ -n "$violations" ]; then
    printf '%s\n' "$violations"
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "inline-tests: OK (${#files[@]} files scanned)"
