<!-- FilePath: AGENTS.md -->

# Simple Voice - Contributor Guide

Tauri 2 dictation app for macOS on Apple Silicon: a Rust workspace (`src-tauri/`, `crates/*`),
a React/TypeScript UI (`src/`), and a Swift engine helper (`engine/`) for everything that needs
Apple frameworks. Published through Homebrew for other people to use.

## Read first

`docs/` holds user documentation; everything for contributors lives in `docs/internal/`.

- `docs/internal/requirements.md`: every requirement, with IDs. A feature is done only when it
  satisfies its entries. Add or change an entry whenever a requirement is added or changed.
- `docs/internal/architecture.md`: the contract - crate map and public APIs, Tauri commands,
  events, windows, the engine-helper protocol, the database schema and storage layout. Code to
  it; when the contract must change, change the document in the same commit.
- `docs/internal/design.md`: Signal, the UI design language - tokens, type, layout, components,
  themes. UI work follows it; styles read colours only from `src/styles/tokens.css`.
- `docs/internal/development.md`: gates, the sqlx offline workflow, engine build.
- `docs/internal/releasing.md`: versioning, signing, the Homebrew cask and tap.
- `docs/internal/engine.md`: the Swift helper's protocol, models and permissions.

## Inviolable

- Rust forbids `unsafe`: `#![forbid(unsafe_code)]` at the top of every crate root and
  `[lints] workspace = true` in every member manifest. Code that needs Objective-C or C APIs
  belongs in the Swift helper.
- No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` or `dbg!` outside `#[cfg(test)]`.
- No suppressions: no `#[allow(...)]` / `#[expect(...)]`, eslint-disable comments, `@ts-ignore`,
  `@ts-expect-error`, or `swiftlint:disable`. Fix the code, or change the lint configuration
  with a reason.
- SQL only through the sqlx macros (`query!`, `query_as!`, `query_scalar!`, `migrate!`) with the
  committed `.sqlx/` cache. No runtime `sqlx::query(`, `QueryBuilder`, or raw `.execute("...")`.
- Migrations in `crates/sv-storage/migrations/` are forward-only and immutable once shipped.
- Dependencies are declared once in the root `[workspace.dependencies]`; crates use
  `name.workspace = true`.
- Every source file starts with its own path: `// FilePath: <path>` (Rust, TS, JS, Swift),
  `/* FilePath: <path> */` (CSS), `-- FilePath: <path>` (SQL), `# FilePath: <path>` (TOML,
  shell, just, YAML, Ruby, hooks), `<!-- FilePath: <path> -->` (Markdown); after a shebang or
  `// swift-tools-version` line. JSON has none.
- 4-space indentation everywhere; 100 columns for Rust and TypeScript, 120 for Swift.
- At most 500 production lines per file (Rust tests after `#[cfg(test)]` excluded). Limits in
  `quality.toml` may only tighten.
- Rust tests live in one `#[cfg(test)] mod tests { ... }` block at the bottom of the file they
  cover; no `tests/` directories or separate test files.
- No TODO/FIXME/XXX/HACK markers, stubs, placeholders or mock data. Finish the work.
- No hard-coded personal paths, keys or vocabulary: the app is for other people too.
- Never commit a secret. Groq keys (`gsk_...`) are blocked explicitly.
- `scripts/guard/*.sh` enforce these rules; never weaken or bypass them, and never use
  `--no-verify`.

## Organisation

- Crates split horizontally by responsibility (`sv-domain`, `sv-storage`, `sv-text`,
  `sv-cloud`, `sv-audio`, `sv-engine`, the app); dependencies point strictly down the table in
  `docs/internal/architecture.md`.
- Inside a crate and in the UI, code is split vertically by feature (`features/<feature>/`).
- Shared types live in `sv-domain`; do not duplicate them. The UI's view of the contract is
  `src/lib/api.ts`.
- Comments explain why, not what.

## Commands

- `just`: list recipes. `just setup`: first-time setup. `just dev`: run the app.
- `just fmt` writes formatting; `just fmt-check` checks it.
- `just lint`, `just test`, `just guard [name]`: the individual gates.
- `just check`: the full gate (fmt-check, guard, lint, test, sqlx-check). It must pass before a
  push; pre-push runs it.
- `just sqlx-prepare` after adding or changing any query macro; commit the `.sqlx/` changes.
- Verify narrowly while iterating (`cargo test -p <crate>`, `just guard <name>`); run the full
  gate once at the end.

## Commits

- One line: `<emoji> <Imperative verb> ...`, at most 72 characters, no trailing period, no body,
  e.g. `✨ Add hold-to-speak shortcut`, `🐛 Fix paste order for overlapping dictations`.
- No co-authorship trailers or generated-by notes.
- Pre-commit runs the guards, format checks and typos on staged files; pre-push runs
  `just check`.
