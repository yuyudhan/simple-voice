<!-- FilePath: docs/development.md -->

# Simple Voice - Development

How to build, check and release Simple Voice. The rules themselves are summarised in
`AGENTS.md`; the contract between the pieces is `docs/architecture.md`.

## Setup

Install the prerequisites listed in the README (Xcode 26+, rustup, bun, just, sqlx-cli with the
SQLite driver, taplo, typos, shellcheck, gitleaks), then:

```sh
just setup     # bun install, core.hooksPath -> .githooks, toolchain report (just doctor)
just dev       # debug engine helper + `bun run tauri dev`
```

sqlx-cli must be built with SQLite support, or `sqlx database create` fails with "no driver
found for URL scheme sqlite":

```sh
cargo install sqlx-cli --no-default-features --features sqlite,rustls --locked
```

## Quality gates

`just check` is the full gate. Pre-push and CI run exactly this, serially:

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
  period, no co-authorship or generated-by notes.
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

`engine/` is a SwiftPM executable that Tauri bundles as a sidecar. `just engine` (or
`bash engine/build.sh debug|release`) builds it and copies it to
`src-tauri/binaries/simple-voice-engine-aarch64-apple-darwin`, where Tauri expects it. Every
recipe that compiles `src-tauri` builds the debug helper first if it is missing, because the
Tauri build script refuses to run without it. `just test` also runs `swift build` on the package.

Building needs the macOS 26 SDK (Xcode 26+) for SpeechAnalyzer and FoundationModels; the
binary links FoundationModels weakly and still runs on macOS 14.

## Permissions during development

macOS remembers Microphone, Accessibility and Speech Recognition grants per code signature.
Development builds are ad-hoc signed, and their signature changes with every rebuild, so macOS
may forget a grant or show a stale entry that no longer matches:

- If recording or pasting stops working after a rebuild, open System Settings → Privacy &
  Security, remove Simple Voice (and `simple-voice-engine`, if listed) from the affected list,
  then grant it again from the app's Settings → Permissions.
- `tccutil reset Accessibility dev.yuyudhan.simplevoice` (or `Microphone`,
  `SpeechRecognition`) clears a grant from the command line.
- Launch the app from `just dev` or the built bundle, not from a terminal that already holds its
  own grants, or macOS attributes the permission to the terminal instead.

Release builds signed with a Developer ID keep a stable identity, so grants survive upgrades.
Ad-hoc signed releases behave like development builds; the cask's caveat explains this to users.

## Releasing

1. Make sure `main` is green and the working tree is clean.
2. `just release 0.2.0` sets the version in `package.json`, `src-tauri/tauri.conf.json` and the
   root `Cargo.toml`, refreshes `Cargo.lock`, commits `🔖 Release v0.2.0`, and tags `v0.2.0`.
3. `git push origin HEAD v0.2.0`.

The tag starts `.github/workflows/release.yml`, which:

1. checks that the tag matches every version field;
2. builds the engine helper and `bun run tauri build --target aarch64-apple-darwin`;
3. signs with a Developer ID and notarizes when the signing secrets are configured, and signs
   ad hoc (`APPLE_SIGNING_IDENTITY=-`) otherwise;
4. uploads `Simple-Voice_<version>_aarch64.dmg` and its `.sha256` to the GitHub release;
5. runs `scripts/release/update-cask.sh publish`, which renders
   `packaging/homebrew/Casks/simple-voice.rb` with the version and checksum and pushes it to
   `yuyudhan/homebrew-tap`.

### Repository secrets

| Secret                                        | Purpose                                                                           |
| --------------------------------------------- | --------------------------------------------------------------------------------- |
| `HOMEBREW_TAP_TOKEN`                          | Required. Fine-grained token with Contents read/write on `yuyudhan/homebrew-tap`. |
| `APPLE_CERTIFICATE`                           | Optional. Base64 of the Developer ID Application `.p12`.                          |
| `APPLE_CERTIFICATE_PASSWORD`                  | Password of that `.p12`.                                                          |
| `APPLE_SIGNING_IDENTITY`                      | e.g. `Developer ID Application: Name (TEAMID)`.                                   |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization: Apple ID, app-specific password, team ID.                           |

Without the Apple secrets the release is ad-hoc signed and the cask clears the download
quarantine in a `postflight` step, with a caveat telling users why. With them, that block is
removed from the published cask. See `packaging/homebrew/README.md` for the tap itself.
