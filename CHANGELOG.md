<!-- FilePath: CHANGELOG.md -->

# Changelog

Release notes for each version are also on the
[GitHub releases page](https://github.com/yuyudhan/simple-voice/releases).

## v0.4.0 - 2026-11-09

### Added

- A banner at the top of the window for every problem that stops or degrades dictation. Each one
  says in plain words what is wrong and has a button for the fix, and goes away by itself once the
  problem is fixed. The problems covered are: no microphone; the chosen microphone missing,
  unplugged, muted or sending no audio; the microphone busy or failing to open; microphone or
  Accessibility access off; the speech engine stopped or not responding; the voice model missing,
  downloading, unavailable or failing; the Groq key missing or refused, its rate limit and no
  network; a shortcut taken by another app; and on Linux, no PipeWire/PulseAudio, no paste tool,
  or Wayland's missing global shortcuts. The buttons open the exact System Settings pane, choose
  a microphone, check again, restart the engine, download the model, or open provider or
  shortcut settings.
- The menu bar menu and its tooltip show the most important problem; choosing it opens the
  window.
- [Troubleshooting](docs/troubleshooting.md): what each banner means and how to fix it.

### Fixed

- A computer without a microphone (a Mac mini, a desktop without a headset) no longer waits
  silently forever when you dictate. Simple Voice refuses to start the dictation and says that
  no microphone is connected.
- Unplugging the microphone during a dictation stops the recording at once and transcribes what
  was heard up to that point, instead of hanging.
- On Linux, a failing or missing audio device no longer floods the log with tens of thousands of
  ALSA errors a minute. The failing stream (microphone capture or the sound cues' output) is
  closed, the error is logged once, and the app reports the problem.
- `simple-voice --start` with no usable microphone prints why (the banner's words, such as "No
  microphone connected") and exits with status 1, instead of doing nothing.
- On Linux, plugging in or pulling out a microphone (or changing the default input) shows or
  clears the "No microphone connected" banner at once instead of at the next check a few seconds
  later.
