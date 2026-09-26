<!-- FilePath: docs/getting-started.md -->

# Getting started

Install Simple Voice with Homebrew or the install script as described in the
[README](../README.md#install), then open it from Applications or Spotlight.

## Permissions

On first launch a short onboarding asks for three macOS permissions. Settings → Permissions shows
their live status at any time and links straight to the right pane of System Settings.

| Permission             | Why it is needed                                                                                            |
| ---------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Microphone**         | To record what you say.                                                                                     |
| **Accessibility**      | To paste the result into the app you are using (a synthetic ⌘V). Without it, text is copied but not pasted. |
| **Speech Recognition** | Only for the Apple Speech engine.                                                                           |

If a permission is missing when you dictate, Simple Voice tells you which one and how to grant
it rather than failing silently.

## Groq API key

If you use Groq, paste your API key in Settings → Models (one key serves both transcription and
formatting). Create one at [console.groq.com](https://console.groq.com/keys). The `GROQ_API_KEY`
environment variable is used when no key is saved. On-device engines need no key.

## Updates

Simple Voice checks once a day for a new release. When one is out, a banner appears above every
page and the menu bar icon shows "Update Available". Copy the command it offers and run it in
Terminal:

```sh
curl -fsSL -o /tmp/simple-voice-install.sh https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh && bash /tmp/simple-voice-install.sh
```

It updates through Homebrew when Homebrew installed Simple Voice (the same as
`brew upgrade --cask simple-voice`), and straight from GitHub otherwise. It quits Simple Voice
while it updates and reopens it afterwards. Dismissing the banner skips that version only.
Settings → System → Updates shows the running version, checks on demand, and turns the daily
check off.

## After an upgrade

macOS ties permissions to the app's code signature. If dictation stops recording or pasting
after an update, open System Settings → Privacy & Security, remove Simple Voice from the
affected list, and grant it again from the app's Settings → Permissions.
