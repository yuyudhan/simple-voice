<!-- FilePath: README.md -->

<p align="center">
    <img src="branding/icon.png" width="128" height="128" alt="Simple Voice icon">
</p>

<h1 align="center">Simple Voice</h1>

<p align="center">
    Speak anywhere on your Mac. Clean, well-formatted text lands in whatever you are typing into.
</p>

<p align="center">
    <a href="https://github.com/yuyudhan/simple-voice/releases/latest"><img src="https://img.shields.io/github/v/release/yuyudhan/simple-voice?label=release" alt="Latest release"></a>
    <a href="https://github.com/yuyudhan/simple-voice/actions/workflows/ci.yml"><img src="https://github.com/yuyudhan/simple-voice/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
    <img src="https://img.shields.io/badge/macOS-14%2B%20%C2%B7%20Apple%20Silicon-black" alt="macOS 14+ on Apple Silicon">
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center">
    <a href="#install">Install</a> · <a href="docs/features.md">Features</a> · <a href="docs/getting-started.md">Getting started</a> · <a href="docs/privacy.md">Privacy</a>
</p>

![Holding fn while saying "um so hi ankur, the launch is on friday, no wait, thursday", then the cleaned-up email pasted into Mail](docs/images/demo.gif)

## Install

Requires an Apple Silicon Mac with macOS 14 (Sonoma) or later.

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

The script checks the download against the release's SHA-256 checksum and the app's code signature before it replaces `~/Applications/Simple Voice.app`; no administrator password is needed. To install a specific release, use `... | bash -s -- --version X.Y.Z`.

Then open **Simple Voice** from Spotlight or `~/Applications` and grant the permissions it asks for: Microphone and Accessibility, plus Speech Recognition if you use Apple Speech. See [Getting started](docs/getting-started.md).

| Task                          | How                                                                   |
| ----------------------------- | --------------------------------------------------------------------- |
| Upgrade                       | Click **Install update** in the app, or run the install command again |
| Uninstall                     | `... \| bash -s -- --uninstall`                                       |
| Uninstall and delete all data | `... \| bash -s -- --uninstall --purge`                               |

Uninstalling quits the app, removes it and resets its permissions; your history, dictionary and settings stay in `~/.simplevoice` for a later reinstall. `--purge` deletes them too, including a database you moved in Settings → Privacy & data (the folder itself and any other files in it stay).

Releases are signed with the project's own certificate, so macOS keeps Simple Voice's permissions across upgrades. Until Apple notarizes them, the install script clears the download quarantine so macOS opens the app.

## Why Simple Voice

- **Works everywhere.** Mail, chat, your editor, a terminal: the text is pasted where your cursor is.
- **Cloud or on-device, your pick.** Smart Select picks the model from the languages you speak. Groq in the cloud, or Parakeet, Whisper (Hindi and Hinglish too), Apple Speech and Apple Intelligence on your Mac. Any OpenAI-compatible endpoint, like a local Ollama or LM Studio, works too.
- **Reads like you wrote it.** Fillers and false starts are dropped, lists get laid out, and your dictionary spells names and jargon your way.
- **Yours alone.** No account, no telemetry. History, dictionary and insights stay on your Mac.

![Five spoken phrases turned into clean text: fillers and false starts dropped, shortcuts, jargon, dictionary names and Hinglish](docs/images/cleanup.gif)

See [all features](docs/features.md).

## How it works

![Hold fn and speak, speech to text, clean up, pasted in place](docs/images/how-it-works.gif)

Hold **fn** and speak, let go to paste. Or press **Control+/** to start and stop. Both shortcuts can be changed in Settings, where you can also let Esc cancel a recording (off by default).

To change text you already wrote, select it in any app, press **Option+/**, say how to change it ("make this more formal", "turn this into bullets", "fix the typos") and press **Option+/** again. The rewrite replaces the selection, and Cmd+Z undoes it.

![Selecting a note, pressing Option+/ and saying "make this more formal", then the note rewritten in place](docs/images/edit-by-voice.gif)

|                | Cloud                                   | On-device                                                                                            |
| -------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Speech to text | Groq Whisper (100+ languages)           | Parakeet TDT v3 (25 European languages), Parakeet TDT v2 and Flash Dictation (English), Hinglish Whisper (Hindi, Hinglish), Whisper Turbo (99 languages), Apple Speech |
| Clean up       | Groq, or any OpenAI-compatible endpoint | Apple Intelligence, or a local Ollama or LM Studio                                                   |
| You need       | A Groq API key                          | A one-time model download (250 MB to 3.1 GB); Apple Speech and Apple Intelligence need macOS 26      |

Mix and match: any speech engine works with any clean-up provider, and clean-up can be turned off. If clean-up fails, the plain transcript is pasted, so nothing is lost.

## A look inside

![A tour of the History, Insights, Dictionary, Style and Models pages](docs/images/tour.gif)

- **History.** Every dictation with the time, the app it went to and the text. Search, copy, retry.
- **Insights.** Speaking speed, words, streaks and where you dictate.
- **Dictionary.** Names, jargon and `heard -> written` rules.
- **Style.** Formal or Casual, applied everywhere.
- **Transcription.** Pick a cloud or on-device voice model.

## Your data stays yours

![No account, no telemetry, and everything stored in ~/.simplevoice on your Mac](docs/images/privacy.gif)

Everything lives in `~/.simplevoice` on your Mac: settings, history, dictionary, insights and downloaded models. There is no account and no telemetry. Only what the providers you pick need leaves your Mac: the recording goes to Groq if you use Groq Whisper, and the transcript goes to your clean-up provider, each with your dictionary words as a hint. With an on-device setup, your words never leave your Mac. Read more in [Privacy and your data](docs/privacy.md).

## Build from source

You need macOS 14 or later on Apple Silicon, Xcode 26 or its Command Line Tools (for the macOS 26 SDK), Rust via [rustup](https://rustup.rs), [bun](https://bun.sh), [just](https://just.systems) and sqlx-cli with SQLite support.

```sh
git clone https://github.com/yuyudhan/simple-voice.git
cd simple-voice
just setup     # install dependencies and git hooks
just dev       # run the app
just build     # build the release .app
```

The [development guide](docs/internal/development.md) covers the prerequisites, quality gates and the Swift engine helper.

## Contributing

Issues and pull requests are welcome. Start with the [contributor guide](AGENTS.md) and [docs/internal](docs/internal/), and run `just check` before you push.

## License

[MIT](LICENSE) © 2026 yuyudhan
