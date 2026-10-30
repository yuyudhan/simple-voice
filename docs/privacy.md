<!-- FilePath: docs/privacy.md -->

# Privacy and your data

## What leaves your Mac

By default, nothing you say or write. Every speech, formatting, meeting and translation model
runs on your Mac, and Simple Voice works the same with the network off once its models are
downloaded.

Groq is optional. If you turn it on in Settings with your own Groq API key, the parts you chose
run at Groq instead, and Settings says so next to each Groq choice. Everything else stays local.

| What                        | Local (the default)                                                                                                                                                                 | With Groq chosen                                                                                                                                                                         |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Speech to text              | Nothing. Parakeet, Whisper and Apple Speech run on-device; Apple Speech is always used in on-device-only mode.                                                                      | Settings → Transcription → Transcribe with Groq: each dictation's recording, with your dictionary words as spelling hints, is uploaded to Groq Whisper (`api.groq.com`).                 |
| Formatting, edits, learning | Nothing. Apple Intelligence and the downloaded Qwen3 4B model run on-device.                                                                                                        | Settings → Formatting → Groq: each transcript, your dictionary words, the text you edit by voice and the corrections learning reads are sent to Groq's chat model.                       |
| Context from the text field | Nothing. A short stretch of the focused field is read only when you turned it on, used for that one dictation, and never stored.                                                    | Nothing: while Groq transcribes or formats dictations, no context is read at all.                                                                                                        |
| Meetings                    | Nothing. Recordings, transcripts, speaker names, summaries, chat and translations stay on your Mac.                                                                                 | Meetings → Summary and chat → Groq: the transcript excerpts a summary or an answer needs are sent to Groq. Recordings, transcription, speakers and translations always stay on your Mac. |
| Model downloads             | Only when you click Download: the model files are fetched from Hugging Face (Settings shows the size, license and source first). The request carries nothing about you or your use. | The same.                                                                                                                                                                                |
| Update checks               | A request for the latest release, to GitHub, once a day. It carries the app version and nothing about you or your use.                                                              | The same.                                                                                                                                                                                |

When a Groq request fails, times out or the recording is over Groq's 25 MB upload limit, Simple
Voice falls back to the on-device model you chose, so a dictation still lands. History names the
model that produced the text.

Your Groq API key is checked with Groq when you save it, then kept in the macOS Keychain, never in
the database. Removing it in Settings deletes it from the Keychain; Groq choices then fall back to
the on-device models. Groq's own handling of what you send is governed by your Groq account and
[Groq's privacy policy](https://groq.com/privacy-policy/).

There is no telemetry and no account. Update checks can be turned off in Settings → App. The
name of the app you dictated into is stored in your local history to power Insights; it never
leaves your Mac. History also keeps the text each edit replaced, locally.

## Where your data lives

Everything is stored locally in `~/.simplevoice/`:

| Path              | Contents                                                                                       |
| ----------------- | ---------------------------------------------------------------------------------------------- |
| `simple-voice.db` | Settings, history, dictionary, snippets, meetings and insights (SQLite, readable only by you). |
| `models/`         | Downloaded on-device models.                                                                   |
| `audio/`          | Audio of failed dictations, kept so they can be retried; removed once retried or deleted.      |
| `meetings/`       | Meeting recordings (one folder per meeting); removed when you delete the meeting.              |
| `backups/`        | A copy of the database taken before each upgrade migrates it (the last three are kept).        |

You can move the database to another folder in Settings → Privacy & data; Simple Voice remembers
the new location. To remove everything, run the install command with `--uninstall --purge`:

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash -s -- --uninstall --purge
```

It deletes the app, `~/.simplevoice` and the database in a folder you moved it to, and resets
the app's permissions. Without `--purge` it keeps `~/.simplevoice`.
