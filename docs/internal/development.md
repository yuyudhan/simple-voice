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
just build     # release .app under target/aarch64-apple-darwin/release/bundle/macos/
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

## Transcription eval

`scripts/eval/transcription-eval.sh` runs a folder of labelled clips through transcription models
and prints word error rate, skeleton WER, the share of wrong-script output, speed and the quality
numbers Smart Select gates on (requirement M-11). Clips are `<category>-<name>.wav` (16 kHz mono
PCM16) with the expected text in `<category>-<name>.txt`. The script header defines the scores:
WER folds case, punctuation, nukta and chandrabindu; skeleton WER compares consonant skeletons
that read alike in Devanagari and Roman script, so romanised Hinglish is scored without one
spelling convention winning; category `hi` must contain Devanagari and every other category
must contain none.

`scripts/eval/make-clips.sh` builds one clip folder per dictation language (`en`, `hi`, `hing`)
from `scripts/eval/say-corpus.tsv`, spoken by the macOS voices Rishi and Lekha (plain Hindi,
Hindi with English words, and Indian-accented English), plus `--fleurs N` real Hindi recordings
from Google FLEURS (CC BY 4.0) at a pinned revision. Run each folder with the languages and
prompt the app sends for that choice; record your own dictations for real numbers.

```sh
just engine
scripts/eval/make-clips.sh --out ~/sv-clips --fleurs 150
scripts/eval/transcription-eval.sh --clips ~/sv-clips/hi --languages hi --prompt "" \
    --models whisper-large-v3-turbo --out hi.tsv
scripts/eval/transcription-eval.sh --clips ~/sv-clips/hing --languages hi \
    --prompt "Haan, toh main ab yeh code check karta hoon." --models whisper-hinglish --out hing.tsv
```

Engine models are read from `~/.simplevoice-dev/models`, where `just dev` downloads them
(`--models-dir` to change). The first run of a Whisper model includes its one-time Core ML compile.
To try a candidate, put its WhisperKit folder (with a `.simple-voice-complete` marker) under an
existing model id in another `--models-dir` and name it with `--label`; it then runs through that
id's decoding path.

The run that chose Smart Select's models used 36 macOS `say` clips (English, Hindi and Hinglish
voices, some with white noise) plus two real dictations. Groq Whisper was still measured then;
the app has since become local-only and the eval no longer runs it:

| Model            | English WER | Hindi | Hinglish romanised | Warm time per clip |
| ---------------- | ----------- | ----- | ------------------ | ------------------ |
| Groq Whisper     | 4.3%        | 4.9% WER, Devanagari | 3 of 16      | ~0.4 s (network)   |
| Whisper Turbo    | 4.3%        | 8.2% WER, Devanagari | 3 of 16      | ~1.1 s             |
| Hinglish Whisper | 8.6%        | romanised            | 16 of 16     | ~1.9 s             |
| Parakeet TDT v3  | 4.3%        | unusable             | —            | ~0.1 s             |

Groq's average log-probability stayed above -0.52 on every clip, including wrong ones, which is
why the Whisper floor stays at -1.0 and Whisper writing Hinglish half in Devanagari is caught by
the mixed-script check instead.

The October 2026 language run (190 Hindi clips: 150 FLEURS sentences plus 40 `say` clips, 16 of
them plain Hindi and 24 Hindi with English words; the same audio for Hinglish; 20 English clips)
compared candidates for Hindi and Hinglish on WhisperKit:

| Hindi mode (`hi`)                       | FLEURS WER | Plain Hindi WER | Hindi + English WER | English words in Roman |
| --------------------------------------- | ---------- | --------------- | ------------------- | ---------------------- |
| Whisper Turbo, no prompt (before)       | 38.6%      | 28.4%           | 38.4%               | 27%                    |
| Whisper Turbo with the Hindi lead (now) | 38.6%      | 27.8%           | 21.8%               | 73%                    |
| IndicWhisper Hindi (AI4Bharat, medium)  | 15.7%      | 14.9%           | 42.5%               | 0%                     |

| Hinglish mode (`hi-Latn`) | FLEURS skeleton WER | `say` skeleton WER | `say` WER | Warm time per clip |
| ------------------------- | ------------------- | ------------------ | --------- | ------------------ |
| Hinglish Whisper (Prime)  | 16.4%               | 13.6%              | 24.2%     | ~3.6 s             |
| Hindi2Hinglish Apex       | 17.3%               | 13.2%              | 25.0%     | ~1.1 s             |

Neither candidate replaced a catalog model: Apex did not beat Prime, and IndicWhisper writes every
English word in Devanagari, needs a self-hosted Core ML conversion and is about twice as slow.
In their own modes every model wrote the chosen script on at least 96% of clips. Whisper Turbo
returned empty text for 2–6 of the 150 FLEURS clips per run (a repetition loop that WhisperKit's
fallbacks drop), so its FLEURS WER moves by several points between identical runs. Whisper
Turbo wrote Devanagari for 81–100% of Hinglish clips even with the Hinglish lead; the
wrong-script retry hands such results to Hinglish Whisper.

## Permissions during development

macOS remembers Microphone, Accessibility, Speech Recognition and Screen & System Audio Recording
grants per code signature.
Development builds are ad-hoc signed, and their signature changes with every rebuild, so macOS
may forget a grant or show a stale entry that no longer matches:

- If recording or pasting stops working after a rebuild, open System Settings → Privacy &
  Security, remove Simple Voice (and `simple-voice-engine`, if listed) from the affected list,
  then grant it again from the app's Settings → Privacy & data.
- `tccutil reset Accessibility dev.yuyudhan.simplevoice` (or `Microphone`,
  `SpeechRecognition`) clears a grant from the command line.
- `just dev` merges `src-tauri/tauri.dev.conf.json` and runs the debug engine helper, which
  embeds `engine/Info.dev.plist`. A development run is therefore named "Simple Voice Dev", with
  the identifiers `dev.yuyudhan.simplevoice.dev` (app) and `dev.yuyudhan.simplevoice.dev.engine`
  (helper), so it never shares an identity with the installed Simple Voice. Release builds
  (`just build`) keep "Simple Voice" and `dev.yuyudhan.simplevoice`. Debug builds also report
  `dev: true` from `app_info`, and the sidebar shows "(dev)" in place of the version,
  so the two windows are distinguishable side by side.
- Debug builds keep their data in `~/.simplevoice-dev` instead of `~/.simplevoice`: database,
  `location`, backups, audio, models and `update.log`. A migration added in development
  therefore never upgrades the installed release's database past the schema that release can
  open. A development run starts empty and downloads its own models; to reuse the release's,
  `cp -c -R ~/.simplevoice/models ~/.simplevoice-dev/` clones them without extra disk on APFS.
  The install script's `--purge` never touches `~/.simplevoice-dev`.
- `just dev` runs the bare binary, not an app bundle, so macOS attributes its permissions to
  the terminal that started it (with tmux, the terminal that started the tmux server). Grant
  Accessibility and Microphone to that terminal, then restart `just dev`; a "Simple Voice" entry
  in System Settings has no effect on a development run. `swift -e 'import ApplicationServices;
print(AXIsProcessTrusted())'` run from the same terminal prints `true` once paste will work.
- Only the built bundle, opened from Finder or `open`, is attributed to Simple Voice itself.

Releases are signed with the project's own certificate (see
[releasing.md](releasing.md#signing-and-notarization)), so grants survive upgrades; only local
ad-hoc builds behave as described above.
