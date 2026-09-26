<!-- FilePath: docs/internal/releasing.md -->

# Simple Voice - Releasing and distribution

Simple Voice is distributed only through an install script attached to every GitHub release:

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

Releases are built and published from a maintainer's Mac; no GitHub runner is involved.

## Prerequisites

- Everything in [development.md](development.md#setup).
- `gh auth login` with push access to `yuyudhan/simple-voice`; the GitHub release uses these
  credentials.
- The release signing certificate in your keychain (see
  [Signing and notarization](#signing-and-notarization)).
- Optionally, a Developer ID for notarization.

## Cutting a release

From a clean working tree on `main`, not behind `origin/main`:

```sh
just release 0.2.0    # scripts/release/release.sh
```

which, in order:

1. sets `0.2.0` in `package.json`, `src-tauri/tauri.conf.json` and the root `Cargo.toml`, and
   refreshes `Cargo.lock`;
2. runs `scripts/release/build.sh`: `tauri build --bundles app` (its `beforeBuildCommand` builds
   the release engine helper), verifies the signature, and packs the bundle with `ditto` into
   `target/release-assets/0.2.0/Simple-Voice_0.2.0_aarch64.zip`;
3. commits `🔖 Release v0.2.0`, tags `v0.2.0`, and pushes the branch and tag (the pre-push hook
   runs `just check`);
4. runs `scripts/release/publish.sh`, which uploads the zip, its `.sha256` and
   `scripts/install.sh` to the GitHub release `v0.2.0`.

If anything fails before step 3, the version files are restored and nothing is committed. If
the push or publish fails after the commit, fix the cause, push if needed, then
`just publish 0.2.0` (`scripts/release/publish-tag.sh`): it builds the pushed tag from a clean
detached worktree, so uncommitted or later work never ships, and publishes it.

Users get the release by clicking Install update in the app or by running the install command
again; both follow the latest GitHub release. The asset name is hyphenated because GitHub
rewrites spaces in asset names; the script spells that name exactly. The download is a zip made
by `ditto`, which preserves the bundle's signature.

## The install script

`scripts/install.sh` is uploaded with every release, so
`releases/latest/download/install.sh` always serves the newest one. It downloads the release
zip, checks it against the `.sha256` and the bundle's code signature, and replaces
`~/Applications/Simple Voice.app` (quitting and reopening the app if it was running, and
clearing the quarantine flag). Installing into the user's own Applications folder means no
administrator password, so standard users can install and update. A copy an older script put in
`/Applications` is removed when the user may delete it; otherwise the script says it remains.
`--version X.Y.Z` installs a specific release; it goes after `bash -s --` in the piped command.
Piping is safe because the script does all its work in `main`, called on its last line, so bash
has read the whole file before anything runs.

To change the script without cutting a release, upload it to the latest release:
`gh release upload vX.Y.Z scripts/install.sh --clobber`.

Running apps notice the release on their own: within a day (or at once with Settings → System →
Check now) they read the same latest GitHub release, show an update banner and a menu bar item,
and install it with the latest `install.sh` when the user clicks Install update (see
[architecture.md § 8](architecture.md#8-update-notices)). Pre-releases
(`gh release edit --prerelease`) are never announced.

## Signing and notarization

macOS keeps Microphone, Accessibility and Speech Recognition grants against the app's
designated requirement. An ad-hoc signature has no certificate, so that requirement is the
binary's hash: every upgrade then looks like a new app, System Settings keeps showing the old,
now useless toggle, and users have to remove and re-grant each permission. Releases are
therefore never ad-hoc signed. They are signed with a self-signed certificate, which costs
nothing and needs no Apple account; the designated requirement becomes
`identifier "dev.yuyudhan.simplevoice" and certificate leaf = H"..."`, which every release
shares, so grants survive upgrades. `scripts/release/build.sh` refuses to build without the
certificate and refuses a bundle whose designated requirement names no certificate.

Once per maintainer:

```sh
just signing-identity create ~/secure/simple-voice-release.p12   # first time ever
just signing-identity restore ~/secure/simple-voice-release.p12  # on another Mac
just signing-identity show
```

`create` makes the certificate "Simple Voice Release" (RSA 3072, valid for twenty years) in the
login keychain and writes a passphrase-protected backup. Keep the backup and its passphrase in
a password manager, outside this repository: a lost certificate means the next release is
signed with a new one and every user re-grants permissions once. codesign uses the certificate
without it being trusted, so no trust setting is needed.

To sign with a Developer ID instead ($99 a year) and notarize, export these before
`just release`:

| Variable                                      | Purpose                                                 |
| --------------------------------------------- | ------------------------------------------------------- |
| `APPLE_SIGNING_IDENTITY`                      | e.g. `Developer ID Application: Name (TEAMID)`.         |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization: Apple ID, app-specific password, team ID. |

Switching certificates changes the designated requirement, so users re-grant permissions once.

Gatekeeper refuses to open an un-notarized app downloaded with the quarantine flag, so the
install script clears `com.apple.quarantine` from `Simple Voice.app` after it installs.
