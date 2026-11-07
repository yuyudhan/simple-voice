<!-- FilePath: docs/linux.md -->

# Simple Voice on Linux

Simple Voice runs on Debian and Ubuntu (x86-64, a CPU with AVX2: Intel Haswell, AMD Zen or
later). It is the same app as on macOS with a different engine helper: transcription runs on this
computer with Parakeet TDT v3 on sherpa-onnx (or Whisper Turbo on whisper.cpp), and nothing leaves
it unless you choose Groq.

## Install

```sh
curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash
```

The script hands over to `install-linux.sh` from the same release. It installs for your user
only and never asks for your password: it checks the release's tarball against its SHA-256
checksum and unpacks it into `~/.local/opt/simple-voice`, links `~/.local/bin/simple-voice`, and
adds the menu entry and icon under `~/.local/share`. The app uses some system libraries and
tools (WebKitGTK, GTK, the tray indicator, `xdotool`, `xclip`, `wl-clipboard`, `pactl`,
`secret-tool`); when one is missing the script prints the one `sudo apt install ...` command to
run and stops (libraries) or carries on without it (tools). `simple-voice --version` prints
what is installed. Run the same command again to update; add `-s -- --uninstall` (or
`--uninstall --purge` to also delete your data) to remove it. An install from an older release
used a system package; the script then tells you to remove it with `sudo apt remove simple-voice`.

Updates: the app tells you when a new release is out, and its Install button updates it in
place.

## What works

| Feature                       | Linux                                                                                                                  |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| Voice model                   | Parakeet TDT v3 by default (25 European languages, 671 MB download, on the CPU), Parakeet TDT v2 (English) or Whisper Turbo (99 languages incl. Hindi, 574 MB, slower). Hinglish Whisper and Apple Speech are macOS only. |
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

and the key you want (for example Ctrl+/ again). It starts and stops dictation in the running app.
The same works in KDE's and Sway's shortcut settings.

For hold-to-talk from the window manager, bind the key press to `simple-voice --start` and its
release to `simple-voice --stop`. In i3 or Sway (`~/.config/i3/config`, `~/.config/sway/config`):

```sh
bindsym ctrl+slash exec --no-startup-id simple-voice --toggle
# hold Menu to talk
bindsym Menu exec --no-startup-id simple-voice --start
bindsym --release Menu exec --no-startup-id simple-voice --stop
```

Under i3 on X11 the app's own Ctrl+/ already works; a binding takes the key from it, so bind
another key or turn the app's shortcut off.

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
