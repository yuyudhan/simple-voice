<!-- FilePath: docs/internal/engine.md -->

# simple-voice-engine

The Swift helper that Simple Voice runs as a Tauri sidecar. Everything that needs Apple frameworks
or Objective-C / C APIs lives here, so the Rust crates can forbid `unsafe`: Parakeet transcription
(FluidAudio on Core ML), Apple Speech (`SpeechAnalyzer`), Apple Intelligence post-processing
(Foundation Models), model downloads, permission checks, the frontmost app, synthetic Cmd+V,
muting the output device, and the floating overlay pill.

## Protocol

`simple-voice-engine --models-dir <path>` reads one JSON request per line on stdin and writes one
JSON object per line on stdout. The commands, parameters and results are specified in
[architecture.md § 6](architecture.md#6-engine-helper-protocol-engine). In short:

- request `{"id": 7, "cmd": "transcribe", "model": "parakeet-tdt-v3", "wavPath": "..."}`;
- result `{"id": 7, "ok": true, "result": {...}}` or `{"id": 7, "ok": false, "error": "..."}`;
- zero or more `{"id": 7, "event": "progress", "fraction": 0.4, "message": "..."}` before a
  result (downloads).
- notifications `{"cmd": "overlay_state", ...}` carry no `id` and get no reply; they drive the
  overlay pill and are applied on the main thread in the order they arrive.

Requests run concurrently, so a long transcription or download never delays `permissions` or
`paste`; responses can therefore arrive out of order and are matched by `id`. Each line is written
with a single write. Logs go to stderr only: at startup the helper points file descriptor 1 at
stderr and keeps the real stdout private, so output from any framework cannot corrupt the stream.
The helper exits when stdin closes.

## Overlay pill

`Overlay.swift` owns the pill window and `PillView.swift` draws it in SwiftUI. The helper runs as
an accessory application (`NSApplication` with `LSUIElement` in its Info.plist): no Dock icon, no
menu bar, never activated. The pill is a borderless, non-activating `NSPanel` at status-bar level
with `canJoinAllSpaces` and `fullScreenAuxiliary`, which is what lets it float over full-screen
apps; it ignores the mouse and can never become key. The app sends `overlay_state` (the
`DictationState`), `overlay_visible` and ~30 Hz `overlay_level`; the view animates the level bars
once per displayed frame, scaled to 60 Hz so ProMotion displays move them at the same speed.

Quick check from a shell:

```sh
printf '{"id":1,"cmd":"ping"}\n{"id":2,"cmd":"permissions"}\n' \
    | engine/.build/debug/simple-voice-engine --models-dir /tmp/sv-models
```

## Build

```sh
engine/build.sh            # debug
engine/build.sh release    # release
```

The script runs `swift build --arch arm64` and copies the product to
`src-tauri/binaries/simple-voice-engine-aarch64-apple-darwin`, the name Tauri's `externalBin`
expects. That directory is ignored by git.

Requirements: Xcode 26 (or its command line tools) with Swift 6, Apple Silicon. The binary's
deployment target is macOS 14. Apple Speech and Apple Intelligence need macOS 26 and are checked
at runtime; on older systems `model_status` reports `unsupported` with a reason. FoundationModels
is linked weakly so the helper still launches on macOS 14 and 15.

Dependency: [FluidAudio](https://github.com/FluidInference/FluidAudio) pinned to `0.17.4`.

`engine/Info.plist` is embedded into the executable's `__TEXT,__info_plist` section by
the linker. It carries the microphone and speech recognition usage descriptions, which the
permission prompts require even for a helper binary. Debug builds embed `engine/Info.dev.plist`
instead: the same strings under the name "Simple Voice Dev" and the identifier
`dev.yuyudhan.simplevoice.dev.engine`, so a development helper never shares an identity with the
installed app's `dev.yuyudhan.simplevoice.engine`. Release builds (`just build`) keep `Info.plist`.

## Models

Local models are stored under the `--models-dir` the app passes (by default
`~/.simplevoice/models/`):

| Model id          | Source (Hugging Face)                                                | Files kept                                                                                   |
| ----------------- | -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| `parakeet-tdt-v3` | `FluidInference/parakeet-tdt-0.6b-v3-coreml`                         | `Preprocessor`, `Encoder`, `Decoder`, `JointDecisionv3` (`.mlmodelc`), `parakeet_vocab.json` |
| `parakeet-tdt-v2` | `FluidInference/parakeet-tdt-0.6b-v2-coreml`                         | `Preprocessor`, `Encoder`, `Decoder`, `JointDecision` (`.mlmodelc`), `parakeet_vocab.json`   |
| `parakeet-flash`  | `FluidInference/parakeet-realtime-eou-120m-coreml`, folder `1280ms/` | `streaming_encoder`, `decoder`, `joint_decision` (`.mlmodelc`), `vocab.json`                 |

- `download_model` lists the repository through the Hugging Face API
  (`/api/models/<repo>/tree/main?recursive=true`), keeps only the files above and downloads them
  from `/<repo>/resolve/main/<path>` into `<models-dir>/<model-id>.partial/`. Files already
  complete in a leftover `.partial` directory are reused, so an interrupted download resumes.
  When every file is present a `.simple-voice-complete` marker (holding the total size) is written
  and the directory is renamed to `<models-dir>/<model-id>/`.
- `model_status` is `ready` only when the marker exists; `sizeBytes` is the downloaded size.
- `preload` loads a model into memory; loaded models stay resident until `delete_model`, which
  unloads the model and removes both directories.
- Parakeet Flash is a streaming model; for a finished recording the whole file is fed through the
  streaming manager and then flushed.
- `apple-speech` assets belong to macOS. `download_model` installs them for the requested locale
  through `AssetInventory` (default `en-US`; `en` maps to en-US, `hi` to hi-IN) and
  `delete_model` releases the app's locale reservations so macOS can reclaim the space.
- `apple-intelligence` is never downloaded by the app; `model_status` reports whether the system
  model is available and why not (device not eligible, Apple Intelligence turned off, model still
  downloading).

## Permissions (TCC)

The helper is launched by Simple Voice.app, so macOS attributes its permission checks and
prompts to the app (the responsible process), not to the helper binary:

- `permissions` reads microphone (`AVCaptureDevice`), accessibility (`AXIsProcessTrusted`) and
  speech recognition (`SFSpeechRecognizer`) status for Simple Voice. `AXIsProcessTrusted` can
  keep answering false in a running process after access is granted, so while it does, the
  helper asks a short-lived copy of itself (`simple-voice-engine --check-accessibility`, exit
  status 0 when trusted). The Fn key retry and `paste` use the same check.
- `request_permission` shows the system prompt; for accessibility it opens the prompt that
  leads to System Settings, since that permission can only be granted there.
- `open_settings` opens the matching Privacy & Security pane.
- `paste` fails with `accessibility permission missing` when the app is not trusted.

When the helper is run directly from a terminal, TCC attributes it to the terminal app instead,
so results differ from what the app sees.
