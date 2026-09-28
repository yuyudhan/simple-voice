<!-- FilePath: docs/features.md -->

# Features

Simple Voice is a small, quiet dictation app for macOS on Apple Silicon. Hold a shortcut, talk,
let go: your words are transcribed, tidied up, and pasted into the focused text field of any
app: mail, chat, your editor, a terminal. It stays out of the way in the menu bar and shows a
small floating bar while it listens. Closing the window (Cmd+W) keeps it running in the menu bar,
with no Dock icon, so the shortcuts keep working; Cmd+Q or the menu bar icon's Quit quits it.

- **Dictation anywhere.** Works in any app that accepts text. The result is pasted where your
  cursor is and stays on the clipboard; Settings can restore the previous clipboard instead.
- **Two shortcuts.** _Hold to speak_ records while the keys are held; _toggle to speak_ starts
  on one press and stops on the next. Both are configurable. Esc can cancel a recording once
  you turn that on in Settings; it is off by default so Esc keeps working in your apps.
- **Smart Select (Beta).** Pick the languages you speak; Simple Voice picks the model and offers
  the one download those languages need (Parakeet for English and many European languages,
  Hinglish Whisper for Hinglish, Whisper Turbo for Hindi and other languages). Without a Groq key
  everything stays on your Mac. With a key, Groq Whisper runs first and the local model takes
  over when you're offline or a result looks wrong. If the model can't write one of your
  languages the way you chose offline (Hindi and Hinglish together, for example), it tells you.
- **Or choose the speech engine yourself** (the default).
    - **Groq Whisper** (`whisper-large-v3-turbo`) in the cloud: fast and accurate, needs a Groq
      API key.
    - **Parakeet** on-device models (TDT v3 multilingual, TDT v2 English, and Parakeet Flash),
      downloaded from within the app when you pick them.
    - **Whisper** on-device models: Hinglish Whisper (Hindi and Hinglish, written in Roman script)
      and Whisper Turbo (99 languages).
    - **Apple Speech**, on-device, on macOS 26 and later.
- **Formatting that reads like you wrote it.** A deterministic pass fixes casing, punctuation
  spacing and your personal replacements, then an optional language-model pass drops fillers
  and false starts and lays out lists and steps. Choose the provider:
    - **Groq** (default), fast and remote;
    - **Apple Intelligence**, on-device, on macOS 26 and later (English and other languages
      Apple supports; not Hindi or Hinglish);
    - **any OpenAI-compatible endpoint**, such as a local Ollama or LM Studio, or a hosted service;
    - or **off**.

    Any speech engine works with any formatting provider, so a fully offline setup is one choice
    away. If formatting is slow or fails, the plain transcript is pasted instead; nothing is lost.

- **Edit selected text by voice.** Select text in any input (a Gmail draft, a browser search box,
  a note), press _Toggle to edit_ (⌥/ by default), say how to change it: "make this more
  formal", "turn this into bullets", "fix the typos", then press it again. The rewrite replaces
  the selection in place, and Cmd+Z in that app undoes it. It uses your formatting provider, so
  it needs one that is not off; nothing is changed when nothing is selected or the edit fails.
  Edits appear in History with what you said and the text they replaced. The shortcut is
  configurable in Settings → Dictation and can be turned off.

- **Two styles.** _Formal_ (capitals and full punctuation) or _Casual_ (capitals, lighter
  punctuation), applied everywhere.
- **Languages.** English, Hindi and Hinglish out of the box, each its own choice, on-device
  too. Hinglish is written in Roman script, Hindi in Devanagari; when two choices share a
  language, the picker shows which script each one is written in. The allowed languages are
  configurable.
- **Personal dictionary.** Teach it names, brands and jargon so they are recognised and spelled
  your way, and add replacement rules (`heard -> written`). Import an existing vocabulary file.
- **Learns from your corrections.** Turn on _Learn from your corrections_ in Settings →
  Formatting and fix a misspelled name or term right in the text field after it is pasted: the
  corrected word joins your dictionary, marked _Learned_. Only the changed words, never the rest
  of the field, go to your AI post-processing provider, which decides what is worth learning.
  Works in native apps, Electron apps and Chromium browsers, not in terminals. Delete a learned
  word and it is never learned again. Off by default, and it needs AI post-processing.
- **History.** Every dictation with the time, the app it went to, and the text. Copy, search,
  delete, and retry a failed dictation from its saved audio.
- **Insights.** Words per minute, total words, fixes made, your streak and daily activity, and
  which kinds of apps you dictate into.
- **Floating bar and sounds.** A small pill with live level bars while recording that shows the
  start of what was pasted for 4 seconds afterwards, optional start and stop sounds in a few
  styles, and an option to mute other audio while you speak.
- **Update notices.** A daily check tells you when a new version is out and Install update runs
  the install script, which installs it straight from the GitHub release.
