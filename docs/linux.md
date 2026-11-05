<!-- FilePath: docs/linux.md -->

# Simple Voice on Linux

Simple Voice runs on Debian and Ubuntu (x86-64, a CPU with AVX2: Intel Haswell, AMD Zen or
later). It is the same app as on macOS with a different engine helper: transcription runs on this
computer with Whisper Turbo on whisper.cpp, and nothing leaves it unless you choose Groq.

## Install

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

The script hands over to `install-linux.sh` from the same release. It checks the `.deb` against
the release's SHA-256 checksum and installs it with apt, which asks for your password once (sudo)
and brings in what the app uses: WebKitGTK, the tray indicator, `xdotool`, `xclip`,
`wl-clipboard`, `pactl` and `secret-tool`. `simple-voice --version` prints what is
installed. Run the same command again to update; add `-s -- --uninstall` (or `--uninstall --purge`
to also delete your data) to remove it.

Updates: the app tells you when a new release is out. Installing needs your password, so run the
command above in a terminal; the app's Install button works only when sudo needs no password.

## What works

| Feature                       | Linux                                                                                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Voice model                   | Whisper Turbo (whisper.cpp, 574 MB download, on the CPU). Parakeet, Hinglish Whisper and Apple Speech are macOS only.  |
| Groq (opt-in)                 | Transcription, formatting and meeting summaries with your own key, kept in the Secret Service keyring (GNOME Keyring). |
| Formatting                    | Deterministic formatting (Off) by default, or Groq. Apple Intelligence and the local language model are macOS only.    |
| Edit mode                     | With Groq formatting; the selected text is read from the primary selection.                                            |
| Dictionary, snippets, history | As on macOS.                                                                                                           |
| Meetings                      | Your microphone only; no system audio, no speaker diarization, no live transcript.                                     |
| Mute speakers while recording | Mutes the default output through PulseAudio/PipeWire (`pactl`) and unmutes that same output afterwards.                |
| Launch at login               | An XDG autostart entry (`~/.config/autostart/dev.yuyudhan.simplevoice.desktop`).                                       |
| Floating pill, Fn key, Dock   | macOS only. The tray icon shows the state and starts or stops dictation.                                               |

Features Linux does not have are labelled as such in the app; nothing pretends to work.

## Shortcuts and pasting: X11 and Wayland

Linux keyboards report no Fn key to apps, so dictation starts on **Ctrl+/**: tap to start and
stop, or hold to talk. Edit mode stays on Alt+/.

**X11** (Ubuntu's "Ubuntu on Xorg" login, most other desktops): global shortcuts work everywhere,
and the dictated text is pasted with `xdotool` (Ctrl+V into the focused window).

**Wayland** (Ubuntu's default GNOME session): Wayland gives global shortcuts to no app, so Ctrl+/
only works while an X11 (XWayland) window has focus. Bind a desktop shortcut to the toggle
command instead. In GNOME: Settings → Keyboard → View and Customize Shortcuts → Custom Shortcuts →
add one with the command

```sh
simple-voice --toggle
```

and the key you want (for example Ctrl+/ again). It starts and stops dictation in the running app
(hold-to-talk is not possible this way). The same works in KDE's and Sway's shortcut settings.

Pasting under Wayland needs a tool that can press Ctrl+V for you:

- **GNOME**: `ydotool` with write access to `/dev/uinput`:

    ```sh
    sudo apt install ydotool
    echo 'KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"' |
        sudo tee /etc/udev/rules.d/60-simple-voice-uinput.rules
    sudo usermod -aG input "$USER"
    sudo udevadm control --reload && sudo udevadm trigger
    ```

    then log out and back in.

- **KDE, Sway and other compositors** with the virtual-keyboard protocol: `wtype`
  (`sudo apt install wtype`).

Without one, the text is copied to the clipboard and the app says so; press Ctrl+V yourself.

## Microphone

Linux has no per-app microphone permission. Simple Voice records from the input PulseAudio or
PipeWire calls the default, so choose the microphone in your desktop's sound settings (Settings →
Sound → Input in GNOME); Simple Voice's own microphone list shows only that default. If nothing is
heard, check that the default input is a real microphone and not a "Monitor of …" source.

## Where your data lives

`~/.local/share/simplevoice/` (or `$XDG_DATA_HOME/simplevoice/`), with the same layout as
`~/.simplevoice/` on macOS: the database, `models/`, `audio/`, `meetings/`, `backups/`.
