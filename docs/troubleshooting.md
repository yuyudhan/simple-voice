<!-- FilePath: docs/troubleshooting.md -->

# Troubleshooting

If something stops Simple Voice from hearing you, transcribing or pasting, a banner at the top of
the window tells you what's wrong and has a button for the fix. The same problem also shows at the
top of the menu bar (tray) menu and in its tooltip. Each banner goes away by itself once the
problem is fixed: you plug in a microphone, grant a permission, or the engine comes back.

If a problem means dictation can't work at all (no microphone, microphone access off, the speech
engine down, no voice model), Simple Voice refuses to start a dictation and gives you the reason
straight away instead of recording nothing. On macOS the reason appears in the recording bar. On
Linux, which has no recording bar, the window opens on the banner.

The **Help** button on a banner opens the matching section below. Section names follow the
app's internal name for each problem.

## Microphone

### No microphone

**Banner:** "No microphone connected". Simple Voice can't find any input device. On a Mac mini
or Mac Studio, which have no built-in microphone, this happens until you connect one.

- Connect a microphone (USB, a headset, AirPods), or choose an input in System Settings → Sound →
  Input (macOS) or Settings → Sound → Input (Linux).
- **Open Sound Settings** goes straight to that pane. **Check Again** looks for the microphone
  right away; Simple Voice also checks every few seconds, and on macOS as soon as a device
  connects.

### No sound server

Linux only. Simple Voice records through PipeWire or PulseAudio and neither is running. Start
them with `systemctl --user start pipewire pipewire-pulse` (or log out and back in), then press
**Check Again**. On a desktop without either, install `pipewire-pulse`.

### Selected microphone missing

You chose a microphone in Settings → Dictation → Input device that isn't connected, so Simple Voice
records from another one. Reconnect it, or press **Choose Microphone** and pick another.

### Microphone disconnected

The microphone went away during a recording (unplugged, Bluetooth dropped). The recording stops at
once and Simple Voice transcribes what it heard up to that point. Reconnect it or choose another
input.

### Microphone no audio

The microphone opened but sent no sound at all for 2 seconds, so the recording stopped instead of
waiting forever. This usually means the device is switched off, asleep, or broken. Check it, or
choose another input.

### Microphone silent

The microphone sent only digital silence (every sample exactly zero) for 3 seconds: it is muted
by a switch, or its input volume is at zero. Unmute it, or raise the input level in the sound
settings. A quiet room is never mistaken for this.

### Microphone busy

Another app holds the microphone exclusively. Close that app (often a call or recording app) or
choose another input, then press **Check Again**.

### Microphone failed

The audio system refused to open the microphone for another reason; the banner quotes it. Choose
another input or press **Check Again**. If it persists, restart the computer's audio (on macOS,
unplug and replug the device; on Linux, `systemctl --user restart pipewire pipewire-pulse`).

## Permissions

### Microphone denied

macOS: Simple Voice isn't allowed to use the microphone. **Open Microphone Settings** opens System
Settings → Privacy & Security → Microphone; turn on Simple Voice. Dictation works as soon as you
do, without a restart.

Linux has no microphone permission. If the banner appears there, a sandbox (Flatpak, a container)
or a policy keeps Simple Voice away from the sound server.

### Microphone not asked

macOS hasn't asked yet. **Allow Microphone** shows the macOS prompt; choose Allow.

### Accessibility denied

Without Accessibility access (macOS), Simple Voice can't press ⌘V for you, so dictated text is only
copied: paste it yourself. **Open Accessibility Settings** opens System Settings → Privacy &
Security → Accessibility; turn on Simple Voice. The Fn key shortcut also needs this access.

On Linux the same banner means no paste tool was found: install `xdotool` (X11), or `ydotool`
with write access to `/dev/uinput` (Wayland). See [linux.md](linux.md).

## Speech engine

### Engine down

The on-device speech engine (the `simple-voice-engine` helper) stopped or could not start; the
banner says why. Simple Voice restarts it on its own after a short pause. **Restart Engine** starts it
now. If it keeps failing, reinstall Simple Voice with the install script.

### Engine not responding

The engine is running but did not answer in time (for example a model that hangs while loading).
The banner clears with the engine's next answer. **Restart Engine** if dictations keep failing.

## Voice models

### Model missing

The voice model chosen in Settings → Transcription isn't downloaded. **Download Model** downloads
it; **Choose Model** opens the list to pick another.

### Model downloading

The model is downloading; the banner shows how far along it is. Dictation works as soon as it
finishes.

### Model unavailable

The model can't run on this machine (for example Apple Speech needs a newer macOS, or Apple
Intelligence is turned off). Choose another model.

### Model failed

The model is downloaded but the last dictation with it failed (it didn't load or is damaged).
Delete it in Settings → Transcription and download it again, or **Restart Engine**.

## Cloud provider (Groq)

These only appear when Settings → Transcription is set to Groq. Whenever Groq can't be used,
Simple Voice transcribes on your computer instead, so dictation keeps working.

### Provider key missing

No API key is saved. Add yours in Settings → Transcription (**Open Provider Settings**).

### Provider key invalid

Groq refused the saved key (revoked or mistyped). Paste a valid key in Settings → Transcription.

### Provider rate limited

Groq refuses more requests from your key for now. Wait a minute, or check your plan's limits.

### Provider offline

Simple Voice couldn't reach Groq in time. Check your internet connection. The banner clears after
the next successful request to Groq.

## Shortcuts

### Shortcut unavailable

Another app already uses your dictation shortcut, so Simple Voice can't listen for it. **Change
Shortcut** opens Settings → Dictation; pick another.

### Wayland shortcuts

Wayland desktops don't let apps register global shortcuts. Add a custom shortcut in your
desktop's keyboard settings that runs `simple-voice --toggle` (or `--start`/`--stop` for hold to
talk); see [linux.md](linux.md).
