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

With [Homebrew](https://brew.sh):

```sh
brew install --cask yuyudhan/tap/simple-voice
```

Or with the install script, which uses Homebrew when it is installed and works, and otherwise
installs straight from the latest GitHub release:

```sh
curl -fsSL -o /tmp/simple-voice-install.sh https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh && bash /tmp/simple-voice-install.sh
```

The script checks the download against the release's SHA-256 checksum and the app's code
signature before it replaces `/Applications/Simple Voice.app`. Pass `--no-brew` to skip Homebrew,
or `--version X.Y.Z` to install a specific release. It is the way in when Homebrew is not
available or cannot unpack the app, for example when an endpoint-security agent kills
Homebrew's sandboxed extraction.

Then open **Simple Voice** from Applications and grant the permissions it asks for
(Microphone and Accessibility). See [Getting started](docs/getting-started.md).

| Task                          | Homebrew                                   | Install script                               |
| ----------------------------- | ------------------------------------------ | -------------------------------------------- |
| Upgrade                       | `brew upgrade --cask simple-voice`         | Run the install command again                |
| Uninstall                     | `brew uninstall --cask simple-voice`       | `rm -rf "/Applications/Simple Voice.app"`    |
| Uninstall and delete all data | `brew uninstall --zap --cask simple-voice` | The line above, then `rm -rf ~/.simplevoice` |

Until releases are notarized by Apple, the app is ad-hoc signed; the cask and the install script
both clear its download quarantine so macOS opens it.

### If `brew install` fails

| Error                                                                                                              | Cause and fix                                                                                                                                    |
| ------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Cask 'simple-voice' definition is invalid: undefined method 'run' for an instance of Homebrew::InstallSteps::DSL` | Homebrew is older than 6.0.13. Run `brew update`, then install again, or use the install script.                                                 |
| `sandbox_operation.rb extract` ... `terminated by uncaught signal KILL`                                            | Security software on the Mac (seen with SentinelOne) kills Homebrew while it unpacks the download; every zip cask fails. Use the install script. |

## Documentation

- [Getting started](docs/getting-started.md): permissions, Groq API key, upgrades.
- [Features](docs/features.md)
- [Privacy and your data](docs/privacy.md)
- Contributing: [development guide](docs/internal/development.md) and the rest of
  [docs/internal](docs/internal/).

## License

[MIT](LICENSE) © 2026 yuyudhan
