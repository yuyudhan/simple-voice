<!-- FilePath: packaging/homebrew/README.md -->

# Homebrew packaging

Simple Voice is installed from a personal tap:

```sh
brew install --cask yuyudhan/tap/simple-voice
```

`brew` maps `yuyudhan/tap` to the GitHub repository `yuyudhan/homebrew-tap` and reads the cask
from its `Casks/simple-voice.rb`. This directory holds the template for that file; the release
workflow renders and publishes it, so the tap never needs hand edits.

## One-time setup

1. Create a public GitHub repository named **`yuyudhan/homebrew-tap`** with a `Casks/` directory
   (a README is enough for the first commit; the release adds the cask).
2. Create a fine-grained personal access token limited to that repository with
   **Contents: Read and write**.
3. In `yuyudhan/simple-voice` → Settings → Secrets and variables → Actions, add it as
   **`HOMEBREW_TAP_TOKEN`**.
4. Optionally add the Apple signing and notarization secrets (see `docs/development.md`) so
   releases are Developer ID signed and notarized.

## How a release updates the tap

Pushing a `vX.Y.Z` tag (`just release X.Y.Z`, then push) runs `.github/workflows/release.yml`.
After the DMG is uploaded to the GitHub release as `Simple-Voice_X.Y.Z_aarch64.dmg`, the workflow
runs:

```sh
scripts/release/update-cask.sh publish X.Y.Z <sha256> <signed>
```

which renders `Casks/simple-voice.rb` from the template - pinning `version` and the DMG's
`sha256`, and dropping the quarantine-clearing `postflight` and its caveat when the build was
signed and notarized - then commits and pushes it to the tap. Users get the release with
`brew upgrade --cask simple-voice`; `livecheck` follows the latest GitHub release.

The DMG is published under a hyphenated name because GitHub rewrites spaces in asset names;
the cask URL spells that name exactly.

## Checking a rendered cask locally

```sh
scripts/release/update-cask.sh render 0.1.0 <sha256> false > /tmp/simple-voice.rb
ruby -c /tmp/simple-voice.rb
```

To try it against a real release, copy it into a local tap and install from there:

```sh
brew tap-new --no-git yuyudhan/local
tap="$(brew --repository)/Library/Taps/yuyudhan/homebrew-local"
mkdir -p "$tap/Casks" && cp /tmp/simple-voice.rb "$tap/Casks/"
brew install --cask yuyudhan/local/simple-voice
```

## What the cask does

- Requires Apple Silicon and macOS 14 (Sonoma) or later.
- Installs `Simple Voice.app` and quits the running app before an upgrade or uninstall.
- `brew uninstall --zap` also removes `~/.simplevoice` (database, models, audio, backups) and the
  app's caches, WebKit data, preferences and login item.
