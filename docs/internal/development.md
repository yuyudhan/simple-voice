<!-- FilePath: docs/internal/development.md -->

# Simple Voice - Development

How to build and check Simple Voice. The rules themselves are summarised in `AGENTS.md`; the
contract between the pieces is [architecture.md](architecture.md); releases are covered in
[releasing.md](releasing.md).

## Project layout

| Path                  | What lives there                                                                                                                        |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/`          | The Tauri app: windows, tray, shortcuts, commands and the dictation pipeline.                                                           |
| `crates/`             | Rust libraries split by responsibility: storage, text formatting, cloud clients, audio, the engine client, shared types.                |
| `engine/`             | A Swift helper for everything that needs Apple frameworks: on-device models, Apple Speech, Apple Intelligence, permissions and pasting. |
| `src/`                | The React and TypeScript interface.                                                                                                     |
| `packaging/homebrew/` | The Homebrew cask template.                                                                                                             |
| `docs/`               | User documentation; `docs/internal/` holds these contributor documents.                                                                 |

## Setup

Prerequisites:

- macOS 14 or later on Apple Silicon
- Xcode 26 or later, or its Command Line Tools (Swift 6; the macOS 26 SDK is needed to build the
  Apple Speech and Apple Intelligence support, while the app itself still runs on macOS 14)
- Rust via [rustup](https://rustup.rs)
- [bun](https://bun.sh), [just](https://just.systems)
- sqlx-cli with SQLite support (below)
- For the quality gates: `brew install taplo typos-cli shellcheck gitleaks`

Then:

```sh
git clone https://github.com/yuyudhan/simple-voice.git
cd simple-voice
just setup     # bun install, core.hooksPath -> .githooks, toolchain report (just doctor)
just dev       # debug engine helper + `bun run tauri dev`
just build     # release .app and .dmg under target/aarch64-apple-darwin/release/bundle/
```

sqlx-cli must be built with SQLite support, or `sqlx database create` fails with "no driver
found for URL scheme sqlite":

```sh
cargo install sqlx-cli --no-default-features --features sqlite,rustls --locked
```

## Quality gates

`just check` is the full gate. Pre-push and CI run exactly this, serially; CI also runs
`just cask-check` ([releasing.md](releasing.md#checking-the-cask)):

| Gate       | Recipe            | What it runs                                                                                       |
| ---------- | ----------------- | -------------------------------------------------------------------------------------------------- |
| Formatting | `just fmt-check`  | `cargo fmt --check`, `taplo fmt --check`, `prettier --check`, `swift-format lint --strict`         |
| Guards     | `just guard`      | `scripts/guard/run-all.sh`, every hygiene guard below                                              |
| Lint       | `just lint`       | `cargo clippy --workspace --all-targets -- -D warnings`, eslint, `tsc --noEmit`, typos, shellcheck |
| Tests      | `just test`       | `cargo nextest run` (or `cargo test`) for the workspace, `swift build` for the engine              |
| sqlx cache | `just sqlx-check` | `cargo sqlx prepare --check` against a fresh migrated database                                     |

`just fmt` fixes formatting in place. Rust compiles with `SQLX_OFFLINE=true` in every gate except
the sqlx recipes, so no database is needed for day-to-day work.

Tool configuration: `rustfmt.toml`, `clippy.toml` and `[workspace.lints]` in `Cargo.toml` for
Rust; `taplo.toml` for TOML; `.prettierrc`, `eslint.config.js` and `tsconfig*.json` for the UI;
`engine/.swift-format` for Swift; `.typos.toml` for spelling; `.gitleaks.toml` for secrets.

### Guards

Each guard in `scripts/guard/` checks one rule over `git ls-files`; `just guard <name>` runs one.

| Guard               | Rule                                                                                                                                                                          |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `file-path-headers` | Every source file opens with its `FilePath:` comment (after a shebang or `swift-tools-version` line).                                                                         |
| `file-size`         | At most `[file_size].all` production lines per file (`quality.toml`); Rust test code after `#[cfg(test)]` is not counted.                                                     |
| `forbid-unsafe`     | `unsafe_code = "forbid"` in the workspace lints, `[lints] workspace = true` in every member, `#![forbid(unsafe_code)]` in every crate root, and no `unsafe` keyword anywhere. |
| `inline-tests`      | Rust tests are one `#[cfg(test)] mod tests` block at the bottom of the file; no `tests/` directories.                                                                         |
| `no-runtime-sql`    | No `sqlx::query(`, `query_as(`, `query_scalar(`, `QueryBuilder`, or raw `.execute("...")`.                                                                                    |
| `no-secrets`        | No Groq keys (`gsk_...`) in tracked files; gitleaks over the staged index, the pushed range, or the full history.                                                             |
| `no-suppressions`   | No `#[allow(`/`#[expect(`, eslint-disable, `@ts-ignore`/`@ts-expect-error`/`@ts-nocheck`, or `swiftlint:disable`.                                                             |
| `no-todo`           | No TODO/FIXME/XXX/HACK markers or `todo!`/`unimplemented!`.                                                                                                                   |
| `ratchet`           | `quality.toml` ceilings never loosen compared with `HEAD`, and no new exceptions appear.                                                                                      |
| `workspace-deps`    | Member crates take every dependency from `[workspace.dependencies]`.                                                                                                          |

### Git hooks

`just hooks` points `core.hooksPath` at `.githooks/`:

- **pre-commit**: the guards (gitleaks on the staged index), then rustfmt, taplo and prettier
  checks and typos on the staged files. It never rewrites files; run `just fmt` and re-stage.
- **commit-msg**: one line, `<emoji> <Imperative verb> ...`, at most 72 characters, no trailing
  period, no co-authorship or generated-by notes (`✨ Add hold-to-speak shortcut`).
- **pre-push**: `just check`, with the gitleaks scan narrowed to the commits being pushed. A push
  that adds no new commits (a tag on an already-pushed commit) skips the gate.

## Compile-time checked SQL

Every query is a sqlx macro, checked at compile time against the committed `.sqlx/` cache
(`SQLX_OFFLINE=true`). When you add or change a query or a migration:

```sh
just sqlx-prepare   # recreate target/sqlx-dev.db, apply migrations, regenerate .sqlx/
git add .sqlx
```

`just sqlx-db` alone recreates the throwaway database
(`DATABASE_URL=sqlite:<repo>/target/sqlx-dev.db`) from `crates/sv-storage/migrations/`, which is
handy for pointing an editor's rust-analyzer at a live schema. `just sqlx-check` fails when the
cache and the code disagree; it is part of `just check` and CI. The prepare step covers all
targets, so queries in test code are cached too.

Migrations are forward-only. Never edit a shipped migration; add a new numbered file. The app
backs up the database before applying pending migrations on launch.

## Engine helper

`engine/` is a SwiftPM executable that Tauri bundles as a sidecar; [engine.md](engine.md)
describes its protocol, models and permissions. `just engine` (or
`bash engine/build.sh debug|release`) builds it and copies it to
`src-tauri/binaries/simple-voice-engine-aarch64-apple-darwin`, where Tauri expects it. Every
recipe that compiles `src-tauri` builds the debug helper first if it is missing, because the
Tauri build script refuses to run without it. `just test` also runs `swift build` on the package.

Building needs the macOS 26 SDK (Xcode 26+ or its Command Line Tools) for SpeechAnalyzer and
FoundationModels; the binary links FoundationModels weakly and still runs on macOS 14.

## Permissions during development

macOS remembers Microphone, Accessibility and Speech Recognition grants per code signature.
Development builds are ad-hoc signed, and their signature changes with every rebuild, so macOS
may forget a grant or show a stale entry that no longer matches:

- If recording or pasting stops working after a rebuild, open System Settings → Privacy &
  Security, remove Simple Voice (and `simple-voice-engine`, if listed) from the affected list,
  then grant it again from the app's Settings → Permissions.
- `tccutil reset Accessibility dev.yuyudhan.simplevoice` (or `Microphone`,
  `SpeechRecognition`) clears a grant from the command line.
- `just dev` runs the bare binary, not an app bundle, so macOS attributes its permissions to
  the terminal that started it (with tmux, the terminal that started the tmux server). Grant
  Accessibility and Microphone to that terminal, then restart `just dev`; a "Simple Voice" entry
  in System Settings has no effect on a development run. `swift -e 'import ApplicationServices;
print(AXIsProcessTrusted())'` run from the same terminal prints `true` once paste will work.
- Only the built bundle, opened from Finder or `open`, is attributed to Simple Voice itself.

Release builds signed with a Developer ID keep a stable identity, so grants survive upgrades.
Ad-hoc signed releases behave like development builds; the cask's caveat explains this to users.
