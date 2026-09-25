#!/usr/bin/env bash
# FilePath: scripts/guard/no-todo.sh
# Forbids unfinished-work markers in tracked source: the prose markers TODO, FIXME, XXX and
# HACK, and the Rust macros `todo!` and `unimplemented!`. The workspace lints already deny the
# macros in compiled code; this guard also catches the form that compiles cleanly and quietly
# becomes permanent - a plain comment promising to finish something later. A dictation app
# that runs unattended in the background must not ship known-incomplete paths.
#
# Scope: tracked `.rs`, `.ts`, `.tsx`, `.js`, `.css`, `.sql`, `.swift`, `.sh`, `.just`, `.yml`,
# `.yaml`, `.toml`, `.rb` files, the root `justfile` and the git hooks, excluding this script
# (which names the markers while documenting the rule). Markdown is prose and is not scanned.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$root"

self="scripts/guard/no-todo.sh"

files=()
while IFS= read -r -d '' f; do
    [ "$f" = "$self" ] && continue
    [ -f "$f" ] && files+=("$f")
done < <(git ls-files -z -- '*.rs' '*.ts' '*.tsx' '*.js' '*.css' '*.sql' '*.swift' '*.sh' \
    '*.just' '*.yml' '*.yaml' '*.toml' '*.rb' 'justfile' '.githooks/*')

if [ "${#files[@]}" -eq 0 ]; then
    echo "no-todo: internal error - no tracked source files; this guard measured nothing" >&2
    exit 1
fi

status=0
matches=$(grep -nE '(^|[^A-Za-z0-9_])(TODO|FIXME|XXX|HACK)([^A-Za-z0-9_]|$)|(^|[^A-Za-z0-9_])(todo|unimplemented)!' \
    -- "${files[@]}") || status=$?
if [ "$status" -ge 2 ]; then
    echo "no-todo: internal error - grep failed (exit ${status})" >&2
    exit 1
fi

if [ -n "$matches" ]; then
    while IFS=: read -r file line _rest; do
        echo "${file}:${line}: unfinished-work marker is forbidden - finish the work or track it outside the code"
    done <<<"$matches"
    exit 1
fi

echo "no-todo: OK (${#files[@]} files scanned)"
