<!-- FilePath: README.md -->

<p align="center">
    <img src="branding/icon.png" width="128" height="128" alt="Simple Voice icon">
</p>

<h1 align="center">Simple Voice</h1>

<p align="center">
    Speak anywhere on your Mac. Clean, well-formatted text lands in whatever you are typing into.
</p>

Simple Voice is a small, quiet dictation app for macOS on Apple Silicon. Hold a shortcut, talk,
let go: your words are transcribed, tidied up, and pasted into the focused text field of any
app: mail, chat, your editor, a terminal. It stays out of the way in the menu bar and shows a
small floating bar while it listens.

It aims for few choices, sensible defaults, and a fast path from voice to text, with the option
to keep everything on your machine.

## Features

- **Dictation anywhere.** Works in any app that accepts text. The result is pasted where your
  cursor is, and your clipboard is left as it was.
- **Two shortcuts.** _Hold to speak_ records while the keys are held; _toggle to speak_ starts
  on one press and stops on the next. Both are configurable, and Esc cancels a recording.
- **Your choice of speech engine.**
    - **Groq Whisper** (`whisper-large-v3-turbo`) in the cloud: fast and accurate, needs a Groq
      API key.
    - **Parakeet** on-device models (TDT v3 multilingual, TDT v2 English, and Parakeet Flash),
      downloaded from within the app when you pick them.
    - **Apple Speech**, on-device, on macOS 26 and later.
- **Formatting that reads like you wrote it.** A deterministic pass fixes casing, punctuation
  spacing and your personal replacements, then an optional language-model pass drops fillers
  and false starts and lays out lists and steps. Choose the provider:
    - **Groq** (default), fast and remote;
    - **Apple Intelligence**, on-device, on macOS 26 and later;
    - **any OpenAI-compatible endpoint**, such as a local Ollama or LM Studio, or a hosted service;
    - or **off**.

    Any speech engine works with any formatting provider, so a fully offline setup is one choice
    away. If formatting is slow or fails, the plain transcript is pasted instead; nothing is lost.

- **Two styles.** _Formal_ (capitals and full punctuation) or _Casual_ (capitals, lighter
  punctuation), applied everywhere.
- **Languages.** English, Hindi and Hinglish out of the box: Hinglish stays romanised, pure
  Hindi comes out in Devanagari. The allowed languages are configurable.
- **Personal dictionary.** Teach it names, brands and jargon so they are recognised and spelled
  your way, and add replacement rules (`heard -> written`). Import an existing vocabulary file.
- **History.** Every dictation with the time, the app it went to, and the text. Copy, search,
  delete, and retry a failed dictation from its saved audio.
- **Insights.** Words per minute, total words, fixes made, your streak and daily activity, and
  which kinds of apps you dictate into.
- **Floating bar and sounds.** A small pill with live level bars while recording, optional start
  and stop sounds in a few styles, and an option to mute other audio while you speak.

## Install

Simple Voice needs an Apple Silicon Mac running macOS 14 (Sonoma) or later.

```sh
brew install --cask yuyudhan/tap/simple-voice
```

Upgrade with `brew upgrade --cask simple-voice`. Uninstall with
`brew uninstall --cask simple-voice`; add `--zap` to also remove your history, settings and
downloaded models.

## First run and permissions

On first launch a short onboarding asks for three macOS permissions. Settings → Permissions shows
their live status at any time and links straight to the right pane of System Settings.

| Permission             | Why it is needed                                                                                            |
| ---------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Microphone**         | To record what you say.                                                                                     |
| **Accessibility**      | To paste the result into the app you are using (a synthetic ⌘V). Without it, text is copied but not pasted. |
| **Speech Recognition** | Only for the Apple Speech engine.                                                                           |

If a permission is missing when you dictate, Simple Voice tells you which one and how to grant
it rather than failing silently.

If you use Groq, paste your API key in Settings → Models (one key serves both transcription and
formatting). Create one at [console.groq.com](https://console.groq.com/keys). The `GROQ_API_KEY`
environment variable is used when no key is saved.

## Where your data lives

Everything is stored locally in `~/.simplevoice/`:

| Path              | Contents                                                                                                 |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `simple-voice.db` | Settings, history, dictionary and insights (SQLite, readable only by you). API keys are stored here too. |
| `models/`         | Downloaded on-device models.                                                                             |
| `audio/`          | Audio of failed dictations, kept so they can be retried; removed once retried or deleted.                |
| `backups/`        | A copy of the database taken before each upgrade migrates it (the last three are kept).                  |

You can move the database to another folder in Settings → Data; Simple Voice remembers the new
location.

## Privacy

What leaves your Mac depends only on the providers you choose:

| Choice                 | What is sent, and where                                                                                                 |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Groq Whisper           | The recording and your dictionary words (as a recognition hint), to Groq.                                               |
| Groq formatting        | The transcript and your dictionary words, to Groq.                                                                      |
| Custom endpoint        | The transcript and your dictionary words, to the URL you configure. A local Ollama or LM Studio keeps this on your Mac. |
| Parakeet, Apple Speech | Nothing. Models are downloaded once, then everything runs on-device.                                                    |
| Apple Intelligence     | Nothing. Runs on-device.                                                                                                |
| Formatting off         | Nothing.                                                                                                                |

There is no telemetry and no account. The name of the app you dictated into is stored in your
local history to power Insights; it never leaves your Mac.

## Build from source

Prerequisites:

- macOS 14 or later on Apple Silicon
- Xcode 26 or later (Swift 6; the macOS 26 SDK is needed to build the Apple Speech and Apple
  Intelligence support, while the app itself still runs on macOS 14)
- Rust via [rustup](https://rustup.rs)
- [bun](https://bun.sh), [just](https://just.systems)
- sqlx-cli with SQLite support:
  `cargo install sqlx-cli --no-default-features --features sqlite,rustls --locked`
- For the quality gates: `brew install taplo typos-cli shellcheck gitleaks`

Then:

```sh
git clone https://github.com/yuyudhan/simple-voice.git
cd simple-voice
just setup      # installs JavaScript dependencies and git hooks, checks your toolchain
just dev        # builds the engine helper and runs the app with hot reload
just build      # release .app and .dmg under target/aarch64-apple-darwin/release/bundle/
```

## Project layout

| Path                  | What lives there                                                                                                                        |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `src-tauri/`          | The Tauri app: windows, tray, shortcuts, commands and the dictation pipeline.                                                           |
| `crates/`             | Rust libraries split by responsibility: storage, text formatting, cloud clients, audio, the engine client, shared types.                |
| `engine/`             | A Swift helper for everything that needs Apple frameworks: on-device models, Apple Speech, Apple Intelligence, permissions and pasting. |
| `src/`                | The React and TypeScript interface.                                                                                                     |
| `docs/`               | [Requirements](docs/requirements/requirements.md), [architecture](docs/architecture.md), [development guide](docs/development.md).      |
| `packaging/homebrew/` | The Homebrew cask and how the tap is published.                                                                                         |

Rust code in this repository forbids `unsafe`; everything that needs Objective-C or C APIs lives
in the Swift helper instead. The architecture document is the contract between the pieces.

## Development

```sh
just            # list every recipe
just check      # the full gate: formatting, guards, lint, tests, sqlx cache
```

Git hooks run the fast checks on commit and `just check` on push. Commit subjects are a single
line: an emoji, then an imperative verb, at most 72 characters (`✨ Add hold-to-speak shortcut`).
See [docs/development.md](docs/development.md) for the gates, the compile-time SQL workflow, the
engine build, permissions during development, and the release process.

## License

[MIT](LICENSE) © 2026 yuyudhan
