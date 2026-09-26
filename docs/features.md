<!-- FilePath: docs/features.md -->

# Features

Simple Voice is a small, quiet dictation app for macOS on Apple Silicon. Hold a shortcut, talk,
let go: your words are transcribed, tidied up, and pasted into the focused text field of any
app: mail, chat, your editor, a terminal. It stays out of the way in the menu bar and shows a
small floating bar while it listens. Closing the window or pressing Cmd+Q keeps it running in
the menu bar, with no Dock icon, so the shortcuts keep working; quit from the menu bar icon.

- **Dictation anywhere.** Works in any app that accepts text. The result is pasted where your
  cursor is and stays on the clipboard; Settings can restore the previous clipboard instead.
- **Two shortcuts.** _Hold to speak_ records while the keys are held; _toggle to speak_ starts
  on one press and stops on the next. Both are configurable, and Esc cancels a recording.
- **Your choice of speech engine.**
    - **Groq Whisper** (`whisper-large-v3-turbo`) in the cloud: fast and accurate, needs a Groq
      API key.
    - **Parakeet** on-device models (TDT v3 multilingual, TDT v2 English, and Parakeet Flash),
      downloaded from within the app when you pick them.
    - **Apple Speech**, on-device, on macOS 26 and later.
- **Formatting that reads like you wrote it.** A deterministic pass fixes casing, punctuation
  spacing and your personal replacements, then an optional language-model pass drops fillers
  and false starts and lays out lists and steps. Choose the provider:
    - **Groq** (default), fast and remote;
    - **Apple Intelligence**, on-device, on macOS 26 and later;
    - **any OpenAI-compatible endpoint**, such as a local Ollama or LM Studio, or a hosted service;
    - or **off**.

    Any speech engine works with any formatting provider, so a fully offline setup is one choice
    away. If formatting is slow or fails, the plain transcript is pasted instead; nothing is lost.

- **Two styles.** _Formal_ (capitals and full punctuation) or _Casual_ (capitals, lighter
  punctuation), applied everywhere.
- **Languages.** English, Hindi and Hinglish out of the box: Hinglish stays romanised, pure
  Hindi comes out in Devanagari. The allowed languages are configurable.
- **Personal dictionary.** Teach it names, brands and jargon so they are recognised and spelled
  your way, and add replacement rules (`heard -> written`). Import an existing vocabulary file.
- **History.** Every dictation with the time, the app it went to, and the text. Copy, search,
  delete, and retry a failed dictation from its saved audio.
- **Insights.** Words per minute, total words, fixes made, your streak and daily activity, and
  which kinds of apps you dictate into.
- **Floating bar and sounds.** A small pill with live level bars while recording that shows the
  start of what was pasted for 4 seconds afterwards, optional start and stop sounds in a few
  styles, and an option to mute other audio while you speak.
- **Update notices.** A daily check tells you when a new version is out and Install update runs
  the install script, which installs it straight from the GitHub release.
