<!-- FilePath: docs/internal/releasing.md -->

# Simple Voice - Releasing and Homebrew

Simple Voice is distributed only through a personal Homebrew tap:

```sh
brew install --cask yuyudhan/tap/simple-voice
```

`brew` maps `yuyudhan/tap` to the GitHub repository `yuyudhan/homebrew-tap` and reads the cask
from its `Casks/simple-voice.rb`. `packaging/homebrew/Casks/simple-voice.rb` in this repository
is the template for that file; the release scripts render and publish it, so the tap never needs
hand edits. Releases are built and published from a maintainer's Mac; no GitHub runner is
involved.

## Prerequisites

- Everything in [development.md](development.md#setup), plus Homebrew (for the cask check).
- `gh auth login` with push access to `yuyudhan/simple-voice` and `yuyudhan/homebrew-tap`. Both
  the GitHub release and the tap push use these credentials.
- Optionally, a Developer ID for signing and notarization (see below).

## Cutting a release

From a clean working tree on `main`, not behind `origin/main`:

```sh
just release 0.2.0    # scripts/release/release.sh
```

which, in order:

1. sets `0.2.0` in `package.json`, `src-tauri/tauri.conf.json` and the root `Cargo.toml`, and
   refreshes `Cargo.lock`;
2. runs `scripts/release/check-cask.sh` (see below), so a cask Homebrew would reject is never
   published;
3. runs `scripts/release/build.sh`: `tauri build --bundles app` (its `beforeBuildCommand` builds
   the release engine helper), verifies the signature, and packs the bundle with `ditto` into
   `target/release-assets/0.2.0/Simple-Voice_0.2.0_aarch64.zip`;
4. commits `🔖 Release v0.2.0`, tags `v0.2.0`, and pushes the branch and tag (the pre-push hook
   runs `just check`);
5. runs `scripts/release/publish.sh`, which uploads the zip and its `.sha256` to the GitHub
   release `v0.2.0`, then `scripts/release/update-cask.sh publish`: it renders the cask -
   pinning `version` and the zip's `sha256`, and dropping every block between the
   `unsigned-build` markers when the build was signed and notarized - and commits and pushes it
   to the tap.

If anything fails before step 4, the version files are restored and nothing is committed. If
the push or publish fails after the commit, fix the cause, push if needed, then
`just publish 0.2.0` (`scripts/release/publish-tag.sh`): it builds the pushed tag from a clean
detached worktree, so uncommitted or later work never ships, and publishes it.

Users get the release with `brew upgrade --cask simple-voice`; `livecheck` follows the latest
GitHub release. The asset name is hyphenated because GitHub rewrites spaces in asset names; the
cask URL spells that name exactly. The download is a zip made by `ditto`, which preserves the
bundle's signature, and Homebrew unpacks it natively.

## Signed and unsigned builds

By default the release is ad-hoc signed (`APPLE_SIGNING_IDENTITY=-`). To sign with a Developer
ID in your keychain and notarize, export these before `just release`:

| Variable                                      | Purpose                                                 |
| --------------------------------------------- | ------------------------------------------------------- |
| `APPLE_SIGNING_IDENTITY`                      | e.g. `Developer ID Application: Name (TEAMID)`.         |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization: Apple ID, app-specific password, team ID. |

Gatekeeper refuses to open an un-notarized app downloaded with the quarantine flag, so for an
ad-hoc release the cask's `postflight_steps` clears `com.apple.quarantine` from
`Simple Voice.app` only, and a caveat tells users why and how to re-grant permissions after an
upgrade (macOS ties grants to the code signature, and an ad-hoc signature changes with every
build). A notarized release drops both blocks from the published cask, and grants survive
upgrades.

Homebrew keeps unsigned casks out of `homebrew/cask`, so Simple Voice stays in its own tap until
releases are notarized. Since Homebrew 6.0 a non-official tap must be trusted before its casks
load; installing by the fully qualified name, as the README does, trusts just this cask.

## Checking the cask

```sh
just cask-check    # scripts/release/check-cask.sh
```

renders the cask for both an ad-hoc signed and a notarized release into a throwaway local tap
and runs `brew style` and `brew audit --strict` on each; CI runs it on every push. The template
is indented with two spaces, the only indentation `brew style` accepts (see `.editorconfig`).

To try a rendered cask against a real release, copy it into a local tap and install from there:

```sh
scripts/release/update-cask.sh render 0.1.0 <sha256> false > /tmp/simple-voice.rb
brew tap-new --no-git yuyudhan/local
tap="$(brew --repository yuyudhan/local)"
mkdir -p "$tap/Casks" && cp /tmp/simple-voice.rb "$tap/Casks/"
brew install --cask yuyudhan/local/simple-voice
```

## What the cask does

- Requires Apple Silicon and macOS 14 (Sonoma) or later.
- Installs `Simple Voice.app` and quits the running app before an upgrade or uninstall.
- `brew uninstall --zap` also removes `~/.simplevoice` (database, models, audio, backups) and the
  app's caches, WebKit data, preferences and login item.
