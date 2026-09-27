<!-- FilePath: docs/privacy.md -->

# Privacy and your data

## What leaves your Mac

Only what the providers you choose need:

| Choice                 | What is sent, and where                                                                                                 |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Groq Whisper           | The recording and your dictionary words (as a recognition hint), to Groq.                                               |
| Groq formatting        | The transcript and your dictionary words, to Groq. For an edit, also the text you selected.                             |
| Custom endpoint        | The transcript and your dictionary words (and the selected text of an edit), to the URL you configure. A local Ollama or LM Studio keeps this on your Mac. |
| Parakeet, Whisper, Apple Speech | Nothing. Models are downloaded once, then everything runs on-device.                                           |
| Smart Select           | Without a Groq key, nothing. With a key, the same as Groq Whisper; the on-device model runs when Groq fails or looks unclear. |
| Apple Intelligence     | Nothing. Runs on-device.                                                                                                |
| Formatting off         | Nothing.                                                                                                                |
| Update checks          | A request for the latest release, to GitHub, once a day. It carries the app version and nothing about you or your use.  |

There is no telemetry and no account. Update checks can be turned off in Settings → App. The
name of the app you dictated into is stored in your local history to power Insights; it never
leaves your Mac. History also keeps the text each edit replaced, locally.

## Where your data lives

Everything is stored locally in `~/.simplevoice/`:

| Path              | Contents                                                                                                 |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `simple-voice.db` | Settings, history, dictionary and insights (SQLite, readable only by you). API keys are stored here too. |
| `models/`         | Downloaded on-device models.                                                                             |
| `audio/`          | Audio of failed dictations, kept so they can be retried; removed once retried or deleted.                |
| `backups/`        | A copy of the database taken before each upgrade migrates it (the last three are kept).                  |

You can move the database to another folder in Settings → Privacy & data; Simple Voice remembers
the new location. To remove everything, run the install command with `--uninstall --purge`:

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash -s -- --uninstall --purge
```

It deletes the app, `~/.simplevoice` and the database in a folder you moved it to, and resets
the app's permissions. Without `--purge` it keeps `~/.simplevoice`.
