<!-- FilePath: docs/getting-started.md -->

# Getting started

Install Simple Voice with the install script as described in the
[README](../README.md#install), then open it from Spotlight or `~/Applications`.

## Permissions

On first launch a short onboarding asks for three macOS permissions. Settings → Privacy & data
shows their live status at any time and links straight to the right pane of System Settings.

| Permission             | Why it is needed                                                                                            |
| ---------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Microphone**         | To record what you say.                                                                                     |
| **Accessibility**      | To paste the result into the app you are using (a synthetic ⌘V). Without it, text is copied but not pasted. |
| **Speech Recognition** | Only for the Apple Speech engine.                                                                           |

If a permission is missing when you dictate, Simple Voice tells you which one and how to grant
it rather than failing silently.

## Groq API key

If you use Groq, paste your API key in Settings → Transcription when Groq Whisper or Smart Select
is your voice model, or in Settings → Formatting when Groq formats your text. One key serves both. Create one at
[console.groq.com](https://console.groq.com/keys). The `GROQ_API_KEY` environment variable is used
when no key is saved. On-device engines need no key.

## Updates

Simple Voice checks once a day for a new release. When one is out, a banner appears above every
page and the menu bar icon shows "Update Available". Click **Install update**: Simple Voice runs

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

It installs straight from the GitHub release, checks the download's checksum and code
signature, quits Simple Voice
while it updates and reopens it afterwards. It installs into `~/Applications`, so no
administrator password is needed. If the update fails, the banner says why and the full
output is in `~/.simplevoice/update.log`; running the command above in Terminal does the same
update. Dismissing the banner skips that version only. Settings → App → Updates shows the
running version, checks on demand, installs the update, and turns the daily check off.

If Simple Voice cannot start because its database was created by a newer version, click
**Update Simple Voice** on that screen to install the latest release.

## After an upgrade

macOS ties permissions to the certificate the app is signed with, and every release uses the same
one, so permissions carry over. Upgrading from an older release that was ad-hoc signed is the one
exception: if dictation then stops recording or pasting, open System Settings → Privacy &
Security, remove Simple Voice from the affected list, and grant it again from the app's
Settings → Privacy & data.
