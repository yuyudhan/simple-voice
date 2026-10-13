<!-- FilePath: docs/privacy.md -->

# Privacy and your data

## What leaves your Mac

Nothing you say or write. Every speech, formatting, meeting and translation model runs on your
Mac, and Simple Voice works the same with the network off once its models are downloaded.

| What                   | What is sent, and where                                                                                                 |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Speech to text         | Nothing. Parakeet, Whisper and Apple Speech run on-device; Apple Speech is always used in on-device-only mode.           |
| Formatting, edits, learning | Nothing. Apple Intelligence and the downloaded Qwen3 4B model run on-device.                                       |
| Context from the text field | Nothing. A short stretch of the focused field is read only when you turned it on, used for that one dictation, and never stored. |
| Meetings               | Nothing. Recordings, transcripts, speaker names, summaries, chat and translations stay on your Mac.                     |
| Model downloads        | Only when you click Download: the model files are fetched from Hugging Face (Settings shows the size, license and source first). The request carries nothing about you or your use. |
| Update checks          | A request for the latest release, to GitHub, once a day. It carries the app version and nothing about you or your use.  |

There is no telemetry, no account and no API key. Update checks can be turned off in Settings → App. The
name of the app you dictated into is stored in your local history to power Insights; it never
leaves your Mac. History also keeps the text each edit replaced, locally.

## Where your data lives

Everything is stored locally in `~/.simplevoice/`:

| Path              | Contents                                                                                                 |
| ----------------- | -------------------------------------------------------------------------------------------------------- |
| `simple-voice.db` | Settings, history, dictionary, snippets, meetings and insights (SQLite, readable only by you).           |
| `models/`         | Downloaded on-device models.                                                                             |
| `audio/`          | Audio of failed dictations, kept so they can be retried; removed once retried or deleted.                |
| `meetings/`       | Meeting recordings (one folder per meeting); removed when you delete the meeting.                        |
| `backups/`        | A copy of the database taken before each upgrade migrates it (the last three are kept).                  |

You can move the database to another folder in Settings → Privacy & data; Simple Voice remembers
the new location. To remove everything, run the install command with `--uninstall --purge`:

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash -s -- --uninstall --purge
```

It deletes the app, `~/.simplevoice` and the database in a folder you moved it to, and resets
the app's permissions. Without `--purge` it keeps `~/.simplevoice`.
