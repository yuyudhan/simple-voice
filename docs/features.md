<!-- FilePath: docs/features.md -->

# Features

Simple Voice is a small, quiet dictation app for macOS on Apple Silicon. Hold a shortcut, talk,
let go: your words are transcribed, tidied up, and pasted into the focused text field of any
app: mail, chat, your editor, a terminal. It is local by default: everything runs on your Mac,
and your audio, transcripts, the text around your cursor, meeting recordings, summaries, chat and
translations never leave it. There is no account and no telemetry; the only network traffic is a
model download you start yourself and the daily update check. Groq is optional: with your own
API key you can send dictations to Groq Whisper, formatting to a Groq chat model, or meeting
summaries and chat to Groq (see _Groq (optional)_ below). It stays out of the way in the menu bar and shows a
small floating bar while it listens. Closing the window (Cmd+W) keeps it running in the menu bar,
with no Dock icon, so the shortcuts keep working; Cmd+Q or the menu bar icon's Quit quits it.

- **Dictation anywhere.** Works in any app that accepts text. The result is pasted where your
  cursor is and stays on the clipboard; Settings can restore the previous clipboard instead.
- **Two shortcuts.** _Hold to speak_ records while the keys are held; _toggle to speak_ starts
  on one press and stops on the next. Both are configurable. Esc can cancel a recording once
  you turn that on in Settings; it is off by default so Esc keeps working in your apps.
- **Smart Select (Beta).** Pick the languages you speak; Simple Voice picks the model and offers
  the one download those languages need (Parakeet for English and many European languages,
  Hinglish Whisper for Hinglish, Whisper Turbo for Hindi and other languages). It runs the best
  downloaded model for your languages and, when that one fails or sounds unsure, double-checks
  with another downloaded model that writes the same languages. A result in the wrong script
  (Devanagari when you chose Hinglish, Roman script when you chose Hindi) is redone once on
  another downloaded model that writes the script you chose. If the model can't write one of
  your languages the way you chose (Hindi and Hinglish together, for example), it tells you.
- **Or choose the speech engine yourself** (the default).
    - **Parakeet** on-device models (TDT v3 multilingual, TDT v2 English, and Parakeet Flash),
      downloaded from within the app when you pick them. Parakeet TDT v3 is the default: on
      first launch, setup offers its one-time download (480 MB), and after that dictation works
      offline with nothing leaving your Mac.
    - **Whisper** on-device models: Hinglish Whisper (Hindi and Hinglish, written in Roman script)
      and Whisper Turbo (99 languages).
    - **Apple Speech**, on macOS 26 and later, always on-device (never Apple's servers).
- **Groq (optional).** Off until you turn it on. Add your own Groq API key in Settings (it is
  checked with Groq, then kept in the macOS Keychain) and pick, each on its own:
    - **Transcribe with Groq** (Settings → Transcription): Groq Whisper Large v3 Turbo or Large
      v3 transcribes your dictations and edit instructions. Your dictionary primes it like local
      Whisper, a language outside your choices is re-run in your fallback language, and a result
      in the wrong script for Hindi or Hinglish, or one Groq is unsure of, is redone on your
      on-device model the way Smart Select double-checks. The on-device voice model you chose
      stays chosen: it takes over when Groq fails, times out or the recording is over Groq's
      25 MB limit, and it always transcribes meetings. History names the model that wrote the text.
    - **Groq formatting** (Settings → Formatting): a Groq chat model (default
      `qwen/qwen3.8-27b`, any Groq model ID works) formats dictations, rewrites edits and judges
      corrections for learning. When Groq fails, the local model or Apple Intelligence (whichever
      is ready) formats instead, else the plain transcript is pasted.
    - **Groq for meetings** (Meetings → Summary and chat): summaries and answers come from the
      same Groq model, falling back to the local model. Recordings, transcription, speakers and
      translations stay on your Mac.

    Each Groq choice says next to it what leaves your Mac. While Groq transcribes or formats
    dictations, no context is read from other apps. See [Privacy and your data](privacy.md).

- **Live transcript.** With Parakeet (TDT v3, TDT v2, Flash) or Apple Speech, the words appear
  in the floating bar and the app window while you are still speaking. When you stop, the whole
  recording is transcribed once more for the pasted text, so it is as accurate as without the
  live view and never repeats words. Whisper models only transcribe after you stop. Turn it off in
  Settings → Transcription.
- **Formatting that reads like you wrote it.** A deterministic pass removes hesitation sounds
  (um, uhh, hmm), fixes casing, punctuation spacing and your personal replacements, then an
  optional on-device language-model pass drops other fillers and false starts and lays out lists
  and steps. Choose the model:
    - **Apple Intelligence** (default), built in on macOS 26 and later when Apple Intelligence
      is on (English and other languages Apple supports; not Hindi or Hinglish);
    - **Qwen3 4B Instruct**, a 2.5 GB download (Apache-2.0) that runs on any Apple Silicon Mac
      with llama.cpp;
    - or **off**.

    Any speech engine works with any formatting model, and every on-device combination is fully
    offline.
    If formatting is slow or fails, the plain transcript is pasted instead; nothing is lost.
    Settings show each model's download size, license and source before you download it.

- **Edit selected text by voice.** Select text in any input (a Gmail draft, a browser search box,
  a note), press _Toggle to edit_ (⌥/ by default), say how to change it: "make this more
  formal", "turn this into bullets", "fix the typos", then press it again. The rewrite replaces
  the selection in place, and Cmd+Z in that app undoes it. It uses your formatting model, so
  formatting must not be off; nothing is changed when nothing is selected or the edit fails.
  Edits appear in History with what you said and the text they replaced. The shortcut is
  configurable in Settings → Dictation and can be turned off.

- **Two styles, per app.** _Formal_ (capitals and full punctuation) or _Casual_ (capitals,
  lighter punctuation). Set one style for everything, a style for each kind of app (work chat,
  personal messages, email, documents, AI prompts, code), and a style for a specific app; an
  app's own style wins over its kind's, which wins over the overall style. The style is chosen
  for the app in front when you start speaking, and History shows the one used.
- **Snippets.** Save text you type over and over (an address, a sign-off, a meeting link,
  several lines if you like) under a short spoken trigger such as "my address". Say the trigger
  in a dictation and the saved text is pasted in its place, exactly as saved: matching ignores
  case but only takes whole words, the longest trigger wins, a snippet never expands inside
  another, and the formatting pass cannot rewrite it.
- **Context from the text field (optional).** Turn on _Use the text around the cursor_ and
  Simple Voice reads a short stretch of the focused field when you start speaking, so names
  and terms already written there are spelled the same way. Password fields, password managers
  and any app you exclude are never read, nothing is read without Accessibility access, and the
  text is used for that one dictation only: never saved, logged or sent anywhere. While Groq
  transcribes or formats your dictations, context is not read at all.
- **Code editor context (optional).** In code editors, _IDE context_ adds the file names and
  identifiers visible in the editor (from the window title and the focused editor text), so
  `paymentService` or `invoice_total.rs` come out right. It reads only what the editor shows
  through Accessibility; it never opens files or runs anything. Editors that expose no text
  simply get no hints.
- **Languages.** English, Hindi and Hinglish out of the box, each its own choice, on-device
  too. Hinglish is written in Roman script; Hindi in Devanagari, with the English words you
  mix in usually kept in Roman script ("कल की meeting"). When two choices share a language,
  the picker shows which script each one is written in. The allowed languages are
  configurable.
- **Personal dictionary.** Teach it names, brands and jargon so they are recognised and spelled
  your way, and add replacement rules (`heard -> written`). Import an existing vocabulary file,
  or export yours to a text file you can import again on another Mac.
- **Built-in vocabulary.** Simple Voice already knows its own name and, out of the box, everyday
  tech and programming terms: `github` comes out as `GitHub`, "node js" as `Node.js`. Open
  _Built-in…_ on the Dictionary page to add packs for the work you talk about (web, backend,
  mobile, cloud, data and AI, sales, marketing, business and finance) or switch them off. Your
  own dictionary always comes first and wins over a built-in spelling.
- **Learns from your corrections.** Turn on _Learn from your corrections_ in Settings →
  Formatting and fix a misspelled name or term right in the text field after it is pasted: the
  corrected word joins your dictionary, marked _Learned_. Only the changed words, never the rest
  of the field, go to your formatting model (on-device, or Groq if you chose it), which decides
  what is worth learning.
  Works in native apps, Electron apps and Chromium browsers, not in terminals. Delete a learned
  word and it is never learned again. Off by default, and it needs AI formatting on.
- **Meetings.** Start and stop a meeting recording yourself: your microphone and, if you allow
  it (Screen & System Audio Recording), what the Mac plays, such as the other side of a call.
  After you stop, Simple Voice transcribes it on your Mac with timestamps, tells the speakers
  apart with a local speaker diarization model (a 22 MB download), and lets you name them, fix
  the text, write a summary with action items, ask questions answered only from the transcript
  with links to the moments they come from, and translate it into another language with a
  dedicated translation model (MADLAD-400, a 1.86 GB download), marked as machine translation
  to check against the original lines. **Known limitation:** Hindi translation has not passed
  acceptance testing. Tested models changed names and weekdays and sometimes invented sentence
  content; do not rely on their Hindi output. This does not affect Hindi dictation.
  Long meetings are processed in pieces, and every step can be cancelled. Recordings, transcripts and
  everything written about them stay on your Mac; export a meeting as Markdown, text or JSON.
- **History.** Every dictation with the time, the app it went to, and the text. Copy, search,
  delete, and retry a failed dictation from its saved audio.
- **Insights.** Words per minute, total words, fixes made, your streak and daily activity, and
  which kinds of apps you dictate into.
- **Floating bar and sounds.** A small pill with live level bars while recording that shows the
  start of what was pasted for 2 seconds afterwards (adjustable in Settings), optional start and
  stop sounds in a few styles, and an option to mute other audio while you speak.
- **Update notices.** A daily check tells you when a new version is out and Install update runs
  the install script, which installs it straight from the GitHub release.
