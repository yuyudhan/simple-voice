<!-- FilePath: docs/internal/releasing.md -->

# Simple Voice - Releasing and Homebrew

Simple Voice is distributed only through a personal Homebrew tap:

```sh
brew install --cask yuyudhan/tap/simple-voice
```

`brew` maps `yuyudhan/tap` to the GitHub repository `yuyudhan/homebrew-tap` and reads the cask
from its `Casks/simple-voice.rb`. `packaging/homebrew/Casks/simple-voice.rb` in this repository
is the template for that file; the release workflow renders and publishes it, so the tap never
needs hand edits.

## One-time setup

1. Create a public GitHub repository named **`yuyudhan/homebrew-tap`** with a README (the first
   release adds `Casks/`).
2. Give the release workflow write access to it with an SSH deploy key, which is scoped to that
   one repository and does not expire:

    ```sh
    ssh-keygen -t ed25519 -N "" -C "simple-voice release" -f /tmp/tap-key
    gh repo deploy-key add /tmp/tap-key.pub -R yuyudhan/homebrew-tap --allow-write \
        --title "simple-voice release"
    gh secret set HOMEBREW_TAP_DEPLOY_KEY -R yuyudhan/simple-voice </tmp/tap-key
    rm /tmp/tap-key /tmp/tap-key.pub
    ```

3. Optionally add the Apple signing and notarization secrets below, so releases are Developer ID
   signed and notarized.
4. Make sure GitHub Actions can run for the account: the release is built on a `macos-15`
   runner.

### Repository secrets

| Secret                                        | Purpose                                                                           |
| --------------------------------------------- | --------------------------------------------------------------------------------- |
| `HOMEBREW_TAP_DEPLOY_KEY`                     | Required. Private key of the write-enabled deploy key on `yuyudhan/homebrew-tap`. |
| `APPLE_CERTIFICATE`                           | Optional. Base64 of the Developer ID Application `.p12`.                          |
| `APPLE_CERTIFICATE_PASSWORD`                  | Password of that `.p12`.                                                          |
| `APPLE_SIGNING_IDENTITY`                      | e.g. `Developer ID Application: Name (TEAMID)`.                                   |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization: Apple ID, app-specific password, team ID.                           |

## Cutting a release

1. Make sure `main` is green and the working tree is clean.
2. `just release 0.2.0` sets the version in `package.json`, `src-tauri/tauri.conf.json` and the
   root `Cargo.toml`, refreshes `Cargo.lock`, commits `🔖 Release v0.2.0`, and tags `v0.2.0`.
3. `git push origin HEAD v0.2.0`.

The tag starts `.github/workflows/release.yml`, which:

1. checks that the tag matches every version field;
2. runs `scripts/release/check-cask.sh` (see below), so a cask Homebrew would reject is never
   published;
3. runs `bun run tauri build --target aarch64-apple-darwin`, whose `beforeBuildCommand` builds
   the release engine helper first;
4. signs with a Developer ID and notarizes when the signing secrets are configured, and signs
   ad hoc (`APPLE_SIGNING_IDENTITY=-`) otherwise;
5. uploads `Simple-Voice_<version>_aarch64.dmg` and its `.sha256` to the GitHub release;
6. runs `scripts/release/update-cask.sh publish <version> <sha256> <signed>`, which renders the
   cask - pinning `version` and the DMG's `sha256`, and dropping every block between the
   `unsigned-build` markers when the build was signed and notarized - then commits and pushes it
   to the tap.

Users get the release with `brew upgrade --cask simple-voice`; `livecheck` follows the latest
GitHub release. The DMG is published under a hyphenated name because GitHub rewrites spaces in
asset names; the cask URL spells that name exactly.

## Signed and unsigned builds

Without the Apple secrets the release is ad-hoc signed. Gatekeeper refuses to open an
un-notarized app downloaded with the quarantine flag, so the cask's `postflight_steps` clears
`com.apple.quarantine` from `Simple Voice.app` only, and a caveat tells users why and how to
re-grant permissions after an upgrade (macOS ties grants to the code signature, and an ad-hoc
signature changes with every build). With the secrets, both blocks are removed from the published
cask and grants survive upgrades.

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
