#!/usr/bin/env bash
# FilePath: scripts/guard/no-runtime-sql.sh
# Every SQL statement must go through sqlx's compile-time-checked macros (`query!`,
# `query_as!`, `query_scalar!`, `migrate!`), verified against the committed `.sqlx/` offline
# cache (requirements S-6, Q-4). This guard rejects the runtime-checked forms that compile
# happily and fail only when a user's dictation hits them:
#
#   - the function forms `sqlx::query(`, `query_as(`, `query_scalar(`, their `_with` variants
#     and `raw_sql(` (the macros are spelled with `!`, so they never match);
#   - `QueryBuilder`, which assembles SQL at runtime;
#   - `.execute("...")` / `.execute(r"...")` / `.execute(r#"...")`, a raw SQL string handed
#     straight to an executor.
#
# Scans tracked `.rs` files; whole-line `//` comments are excluded so documentation can name
# the forbidden forms. Operates on `git ls-files -z` output only.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

files=()
while IFS= read -r -d '' f; do
    [ -f "$f" ] && files+=("$f")
done < <(git ls-files -z -- '*.rs')

if [ "${#files[@]}" -eq 0 ]; then
    echo "no-runtime-sql: internal error - no tracked .rs files; this guard measured nothing" >&2
    exit 1
fi

fail=0

# $1 = extended regex, $2 = message.
scan() {
    local pattern="$1" message="$2" raw status matches
    status=0
    raw=$(grep -nE "$pattern" -- "${files[@]}") || status=$?
    if [ "$status" -ge 2 ]; then
        echo "no-runtime-sql: internal error - grep failed (exit ${status})" >&2
        exit 1
    fi
    [ "$status" -eq 0 ] || return 0
    matches=$(printf '%s\n' "$raw" | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' || true)
    if [ -n "$matches" ]; then
        fail=1
        while IFS=: read -r file line _rest; do
            echo "${file}:${line}: ${message}"
        done <<<"$matches"
    fi
}

scan '\bsqlx::(query|query_as|query_scalar|query_with|query_as_with|query_scalar_with|raw_sql)[[:space:]]*\(' \
    "runtime-checked sqlx function is forbidden - use the query!/query_as!/query_scalar! macro"
scan '(^|[^A-Za-z0-9_:!])(query_as|query_scalar|query_with|raw_sql)[[:space:]]*\(' \
    "runtime-checked sqlx function is forbidden - use the query_as!/query_scalar! macro"
scan '\bQueryBuilder\b' \
    "sqlx::QueryBuilder builds SQL at runtime and is forbidden - use a query! macro"
scan '\.execute[[:space:]]*\([[:space:]]*(r#*)?"' \
    "a raw SQL string passed to .execute() is forbidden - use a query! macro"

if [ "$fail" -ne 0 ]; then
    exit 1
fi

echo "no-runtime-sql: OK (${#files[@]} files scanned)"
