<!-- FilePath: README.md -->

<p align="center">
    <img src="branding/icon.png" width="128" height="128" alt="Simple Voice icon">
</p>

<h1 align="center">Simple Voice</h1>

<p align="center">
    Speak anywhere on your Mac. Clean, well-formatted text lands in whatever you are typing into.
</p>

Hold a shortcut, talk, let go: your words are transcribed, tidied up and pasted into the focused
app. Use Groq in the cloud or run fully on-device with Parakeet, Apple Speech and Apple
Intelligence.

## Install

Requires an Apple Silicon Mac with macOS 14 (Sonoma) or later.

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

The script checks the download against the release's SHA-256 checksum and the app's code
signature before it replaces `~/Applications/Simple Voice.app`; no administrator password is needed. To install a specific release,
use `... | bash -s -- --version X.Y.Z`.

If you installed Simple Voice with Homebrew before, run `brew uninstall --cask simple-voice`
(app data in `~/.simplevoice` is kept), then the command above.

Then open **Simple Voice** from Spotlight or `~/Applications` and grant the permissions it asks for
(Microphone and Accessibility). See [Getting started](docs/getting-started.md).

| Task                          | How                                                                        |
| ----------------------------- | -------------------------------------------------------------------------- |
| Upgrade                       | Click **Install update** in the app, or run the install command again      |
| Uninstall                     | `rm -rf ~/Applications/"Simple Voice.app"`                                 |
| Uninstall and delete all data | The line above, then `rm -rf ~/.simplevoice`                               |

Releases are signed with the project's own certificate, so macOS keeps Simple Voice's
permissions across upgrades. Until Apple notarizes them, the install script clears the download
quarantine so macOS opens the app.

## Documentation

- [Getting started](docs/getting-started.md): permissions, Groq API key, upgrades.
- [Features](docs/features.md)
- [Privacy and your data](docs/privacy.md)
- Contributing: [development guide](docs/internal/development.md) and the rest of
  [docs/internal](docs/internal/).

## License

[MIT](LICENSE) © 2026 yuyudhan
