<!-- FilePath: AGENTS.md -->

# Simple Voice - Contributor Guide

Tauri 2 dictation app for macOS on Apple Silicon, published through Homebrew. Rust workspace
(`src-tauri/`, `crates/*`), React/TypeScript UI (`src/`), Swift engine helper (`engine/`) for
everything that needs Apple frameworks.

## Read first

`docs/` holds user documentation; everything for contributors lives in `docs/internal/`.

- `docs/internal/requirements.md`: requirements with IDs. A feature is done only when it
  satisfies its entries; update them whenever a requirement changes.
- `docs/internal/architecture.md`: the contract (crates, Tauri commands, events, windows, engine
  protocol, database schema, storage). Code to it; change it in the same commit when needed.
- `docs/internal/design.md`: the Signal design language. Colours come only from
  `src/styles/tokens.css`.
- `docs/internal/development.md`: gates, sqlx offline workflow, engine build.
- `docs/internal/releasing.md`: versioning, signing, the Homebrew cask and tap.
- `docs/internal/engine.md`: the Swift helper's protocol, models and permissions.

## Inviolable

- Rust forbids `unsafe`: `#![forbid(unsafe_code)]` in every crate root, `[lints] workspace =
true` in every member manifest. Objective-C or C APIs belong in the Swift helper.
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `dbg!` outside `#[cfg(test)]`.
- No suppressions: `#[allow]`, `#[expect]`, eslint-disable, `@ts-ignore`, `@ts-expect-error`,
  `swiftlint:disable`. Fix the code, or change lint configuration with a reason.
- SQL only via sqlx macros (`query!`, `query_as!`, `query_scalar!`, `migrate!`) with the
  committed `.sqlx/` cache. No runtime `sqlx::query(`, `QueryBuilder`, or raw `.execute("...")`.
- Migrations in `crates/sv-storage/migrations/` are forward-only and immutable once shipped.
- Dependencies declared once in root `[workspace.dependencies]`; crates use `workspace = true`.
- Every source file starts with its path: `// FilePath: <path>` (Rust, TS, JS, Swift),
  `/* */` (CSS), `--` (SQL), `#` (TOML, shell, just, YAML, Ruby, hooks), `<!-- -->` (Markdown);
  after any shebang or `// swift-tools-version` line. JSON has none.
- 4-space indentation; 100 columns for Rust and TypeScript, 120 for Swift.
- At most 500 production lines per file (Rust tests excluded). `quality.toml` limits may only
  tighten.
- Rust tests live in one `#[cfg(test)] mod tests` block at the bottom of the file they cover.
- No TODO/FIXME/XXX/HACK markers, stubs, placeholders or mock data.
- No hard-coded personal paths, keys or vocabulary; no secrets (Groq `gsk_...` keys blocked).
- `scripts/guard/*.sh` enforce these rules; never weaken or bypass them, never `--no-verify`.

## Organisation

- Crates split horizontally by responsibility (`sv-domain`, `sv-storage`, `sv-text`,
  `sv-cloud`, `sv-audio`, `sv-engine`, the app); dependencies point strictly down.
- Inside a crate and in the UI, split vertically by feature (`features/<feature>/`).
- Shared types live in `sv-domain`; the UI's view of the contract is `src/lib/api.ts`.
- Comments explain why, not what.

## Commands

- `just setup` first time; `just dev` runs the app; `just` lists recipes.
- `just fmt` / `just fmt-check`; `just lint`, `just test`, `just guard [name]`.
- `just check`: full gate (fmt-check, guard, lint, test, sqlx-check); pre-push runs it.
- `just sqlx-prepare` after changing any query macro; commit `.sqlx/`.
- Iterate narrowly (`cargo test -p <crate>`, `just guard <name>`); full gate once at the end.

## Commits

- One line: `<emoji> <Imperative verb> ...`, max 72 chars, no period, no body,
  e.g. `✨ Add hold-to-speak shortcut`.
- No co-authorship trailers or generated-by notes.
- Pre-commit runs guards, format checks and typos on staged files.
