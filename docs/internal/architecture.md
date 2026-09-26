<!-- FilePath: docs/internal/architecture.md -->

# Simple Voice — Architecture

Simple Voice is a Tauri 2 app with three parts:

| Part          | Path         | Language                                         | Owns                                                                                                                                                                               |
| ------------- | ------------ | ------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Core          | `src-tauri/` | Rust (`#![forbid(unsafe_code)]`)                 | Database, audio capture, dictation pipeline, Groq, shortcuts, windows, tray, sounds                                                                                                |
| Engine helper | `engine/`    | Swift (SwiftPM executable `simple-voice-engine`) | Everything that needs Apple frameworks: Parakeet (FluidAudio, Core ML), Apple Speech (`SpeechAnalyzer`), model downloads, permissions, frontmost app, synthetic Cmd+V, output mute, the floating overlay pill |
| UI            | `src/`       | React + TypeScript (Vite)                        | The main window (`index.html`)                                                                                                                                                     |

Rust forbids `unsafe`, so every Objective-C / C API call lives in the Swift helper. The helper is
bundled as a Tauri sidecar (`bundle.externalBin = ["binaries/simple-voice-engine"]`), started
once at app launch, kept resident (so Core ML models stay loaded), and restarted on crash.

```mermaid
flowchart LR
    K[Global shortcut] --> C[dictation coordinator]
    C --> A[cpal capture 16 kHz mono]
    A --> T{model}
    T -->|groq| G[Groq Whisper]
    T -->|parakeet / apple| E[Swift engine helper]
    G --> F[deterministic format]
    E --> F
    F --> P[Groq LLM polish]
    P --> D[ordered delivery: clipboard + Cmd+V via helper]
    D --> H[(SQLite history)]
```

## 1. Storage

- Data directory: `~/.simplevoice/` (created on launch, mode 0700).
    - `update.log` — output of the last in-app update install (file mode 0600), replaced by each.
    - `simple-voice.db` — default database location (file mode 0600).
    - `location` — optional one-line file holding the absolute path of the directory that
      contains the database when the user moved it (Settings → Data). Absent = default. If that
      directory is missing at launch (e.g. an unmounted drive), `Db::open` fails with a message
      instead of creating an empty database there. `Db::open_at(dir)` treats `dir` as both the
      data directory and the database directory.
    - `models/` — downloaded local models (owned by the engine helper, passed as `--models-dir`).
    - `audio/` — WAVs of failed dictations, kept for retry; deleted when the entry is retried
      successfully or deleted.
    - `backups/` — `simple-voice-<timestamp>-v<N>.db` copies taken before migrations (last 3 kept).
- Migrations: `crates/sv-storage/migrations/NNNN_name.sql`, embedded with `sqlx::migrate!` and
  applied on every launch, so a new app version upgrades the user's database in place.
  Forward-only; never edit a shipped migration. Before applying pending migrations to an existing
  database the core copies it into `backups/`. A database newer than the app (downgrade) refuses
  to open with a clear message instead of corrupting data.
- Every query uses sqlx macros (`query!`, `query_as!`, `query_scalar!`); runtime
  `sqlx::query(` strings are banned by `scripts/guard/no-runtime-sql.sh`. The offline cache
  `.sqlx/` at the workspace root is committed and CI builds with `SQLX_OFFLINE=true`.
  `just sqlx-prepare` regenerates it against `target/sqlx-dev.db`
  (`DATABASE_URL=sqlite:target/sqlx-dev.db`, created by `just sqlx-db`).

### Schema (migration `0001_init.sql`)

```sql
CREATE TABLE settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL                -- JSON-encoded value
);
CREATE TABLE dictionary (
    id          INTEGER PRIMARY KEY NOT NULL,
    phrase      TEXT NOT NULL COLLATE NOCASE UNIQUE,  -- word to recognise / text heard
    replacement TEXT,                                 -- NULL = plain word; else written form
    created_at  INTEGER NOT NULL                      -- unix ms
);
CREATE TABLE history (
    id               INTEGER PRIMARY KEY NOT NULL,
    created_at       INTEGER NOT NULL,   -- unix ms, recording start
    status           TEXT NOT NULL,      -- 'pasted' | 'unformatted' | 'failed' | 'dropped' | 'not_pasted'
    raw_text         TEXT NOT NULL,      -- transcript as returned by the model
    final_text       TEXT NOT NULL,      -- what was pasted (or would have been)
    error            TEXT,
    model            TEXT NOT NULL,      -- model id, e.g. 'groq-whisper'
    language         TEXT,
    style            TEXT NOT NULL,      -- 'formal' | 'casual'
    audio_ms         INTEGER NOT NULL,
    latency_ms       INTEGER NOT NULL,   -- stop → paste
    word_count       INTEGER NOT NULL,
    dictionary_fixes INTEGER NOT NULL,   -- replacement-rule hits
    words_corrected  INTEGER NOT NULL,   -- word-level edit distance raw → final
    app_name         TEXT,
    bundle_id        TEXT,
    app_category     TEXT NOT NULL,      -- see AppCategory
    audio_path       TEXT                -- set only while a failed entry can be retried
);
CREATE INDEX history_created_at ON history(created_at);
```

## 2. Code organization

### Rust workspace (root `Cargo.toml`)

Split horizontally into crates by responsibility; each crate is split vertically by feature.
Dependencies only point down the table. Every crate: `[lints] workspace = true` and
`#![forbid(unsafe_code)]`.

| Crate                | Path                 | Depends on             | Owns                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------- | -------------------- | ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `simple-voice` (app) | `src-tauri/`         | all below              | Tauri wiring. `src/features/<feature>/` vertical slices (`dictation/`, `history/`, `dictionary/`, `insights/`, `settings/`, `models/`, `permissions/`, `updates/`), each holding that feature's commands and logic; `src/platform/` (`shortcuts.rs`, `overlay.rs`, `tray.rs`, `windows.rs`, `engine_process.rs`, `dock.rs`); `src/events.rs` (event names + emit helpers); `src/state.rs`; `src/lib.rs` |
| `sv-storage`         | `crates/sv-storage/` | `sv-domain`            | `paths.rs`, `db.rs` (open, backup, migrate, relocate), `settings.rs`, `history.rs`, `dictionary.rs`, `insights.rs`, `migrations/`                                                                                                                                                                                                                                                                       |
| `sv-text`            | `crates/sv-text/`    | `sv-domain`            | `vocabulary.rs`, `formatting.rs`, `polish.rs` (prompt, guards, timeout) — pure, no I/O                                                                                                                                                                                                                                                                                                                  |
| `sv-cloud`           | `crates/sv-cloud/`   | `sv-domain`, `sv-text` | `groq_whisper.rs`, `chat.rs` (OpenAI-compatible chat for Groq and custom endpoints), `releases.rs` (latest GitHub release)                                                                                                                                                                                                                                                                              |
| `sv-audio`           | `crates/sv-audio/`   | `sv-domain`            | `devices.rs`, `capture.rs` (cpal → 16 kHz mono i16 + level), `wav.rs`, `cues.rs` (4 synthesized themes, rodio)                                                                                                                                                                                                                                                                                          |
| `sv-engine`          | `crates/sv-engine/`  | `sv-domain`            | `client.rs` (request ids, pending map, progress streams), `protocol.rs` (typed commands/results)                                                                                                                                                                                                                                                                                                        |
| `sv-domain`          | `crates/sv-domain/`  | —                      | Shared serde types, `AppError`, `categorize`, `text_stats` (exists; read the source)                                                                                                                                                                                                                                                                                                                    |

### Crate APIs (the contract between slices)

```rust
// ── sv-domain (exists) ─────────────────────────────────────────────────────────────────
// Settings, SettingsPatch, Style, SoundTheme, Theme, PostProcessing, HistoryEntry, NewHistory,
// HistoryStatus, DictionaryEntry, ImportSummary, Insights, DayActivity, CategoryUsage, AppUsage,
// AppCategory + categorize(), ModelInfo/Kind/Provider/Status + model id consts, Permissions,
// PermissionKind/Status, DictationState/Phase, ModelProgress, AppError/AppResult,
// text_stats::{word_count, words_corrected}, UpdateStatus + Release (update notices).
// Theme = System (default) | Light | Dark, wire "system" | "light" | "dark"; Settings.theme and
// SettingsPatch.theme, stored in the `settings` row keyed `theme` like every other field.
// Settings.checkForUpdates (default true) and Settings.skippedUpdate (dismissed release version,
// "" = none) drive the update notice.

// ── sv-storage ──────────────────────────────────────────────────────────────────────────
pub mod paths {
    pub fn data_dir() -> AppResult<PathBuf>;      // ~/.simplevoice (created, 0700)
    pub fn models_dir() -> AppResult<PathBuf>;    // data_dir/models
    pub fn audio_dir() -> AppResult<PathBuf>;     // data_dir/audio
    pub fn backups_dir() -> AppResult<PathBuf>;   // data_dir/backups
    pub fn database_dir() -> AppResult<PathBuf>;  // contents of data_dir/location, else data_dir
}
#[derive(Clone, Debug)] pub struct Db { .. }       // cheap clone; pool swappable for relocate
impl Db {
    pub async fn open() -> AppResult<Db>;                          // dirs, backup, migrate
    pub async fn open_at(dir: &Path) -> AppResult<Db>;             // tests / relocate
    pub async fn relocate(&self, new_dir: &Path) -> AppResult<()>; // VACUUM INTO, write `location`, reopen, remove old
    pub async fn database_path(&self) -> PathBuf;
    pub async fn settings(&self) -> AppResult<Settings>;            // fills derived fields
    pub async fn update_settings(&self, patch: SettingsPatch) -> AppResult<Settings>; // validates ranges
    pub async fn groq_api_key(&self) -> AppResult<Option<String>>;  // stored, else env GROQ_API_KEY
    pub async fn set_groq_api_key(&self, key: Option<String>) -> AppResult<()>;
    pub async fn custom_api_key(&self) -> AppResult<Option<String>>;
    pub async fn set_custom_api_key(&self, key: Option<String>) -> AppResult<()>;
    pub async fn insert_history(&self, entry: NewHistory) -> AppResult<HistoryEntry>;
    pub async fn update_history(&self, id: i64, entry: NewHistory) -> AppResult<HistoryEntry>;
    pub async fn history_entry(&self, id: i64) -> AppResult<Option<HistoryEntry>>;
    pub async fn history_audio_path(&self, id: i64) -> AppResult<Option<String>>;
    pub async fn list_history(&self, query: Option<String>, limit: i64, before_id: Option<i64>)
        -> AppResult<Vec<HistoryEntry>>;                             // newest first
    pub async fn delete_history(&self, id: i64) -> AppResult<Option<String>>; // audio path to remove
    pub async fn clear_history(&self) -> AppResult<Vec<String>>;              // audio paths
    pub async fn dictionary(&self) -> AppResult<Vec<DictionaryEntry>>;        // phrase order
    pub async fn add_dictionary_entry(&self, phrase: String, replacement: Option<String>)
        -> AppResult<DictionaryEntry>;                               // duplicate → InvalidInput
    pub async fn update_dictionary_entry(&self, id: i64, phrase: String, replacement: Option<String>)
        -> AppResult<DictionaryEntry>;
    pub async fn delete_dictionary_entry(&self, id: i64) -> AppResult<()>;
    pub async fn import_vocabulary(&self, text: &str) -> AppResult<ImportSummary>; // bare lines, `a -> b`, `#`
    pub async fn insights(&self, now_ms: i64, utc_offset_minutes: i32) -> AppResult<Insights>;
}

// ── sv-text ─────────────────────────────────────────────────────────────────────────────
pub struct Rule { pub from: String, pub to: String }
pub struct Vocabulary { pub prompt: String, pub terms: Vec<String>, pub rules: Vec<Rule>, pub omitted: usize }
impl Vocabulary { pub fn from_entries(entries: &[DictionaryEntry]) -> Vocabulary; }  // prompt ≤ 850 chars
impl Default for Vocabulary { .. }                                // empty dictionary
pub const PROMPT_MAX_CHARS: usize = 850;
pub struct Formatted { pub text: String, pub rule_hits: u32 }
pub fn format(text: &str, vocabulary: &Vocabulary, style: Style) -> Formatted;
// Post-processing ("polish"). One prompt, three backends:
//   groq   → Groq chat completions (reasoning_effort "none", temperature 0)   — sv-cloud
//   custom → any OpenAI-compatible /chat/completions (Ollama, LM Studio, ...) — sv-cloud
//   apple  → on-device Apple Intelligence via the engine helper `polish` command — app
pub struct PolishPrompt { pub system: String, pub shots: Vec<(String, String)>, pub user: String }
pub fn polish_prompt(text: &str, terms: &[String], style: Style) -> PolishPrompt;
pub fn polish_timeout(text: &str) -> Duration;                    // 2.5 s + 5 ms/word
pub enum PolishOutcome { Polished(String), Skipped(String) }       // reason
pub fn should_skip_polish(text: &str) -> Option<PolishOutcome>;    // Some(Skipped("too short")) < 3 words
pub fn accept_polish(input: &str, output: &str, finished: bool) -> PolishOutcome; // empty / truncated / grew

// ── sv-cloud ────────────────────────────────────────────────────────────────────────────
pub struct Transcript { pub text: String, pub language: Option<String> }
pub async fn groq_transcribe(client: &reqwest::Client, key: &str, wav: Vec<u8>, prompt: &str,
    languages: &[String], fallback_language: &str) -> AppResult<Transcript>;
pub async fn groq_verify_key(client: &reqwest::Client, key: &str) -> AppResult<()>;
pub const GROQ_CHAT_URL: &str = "https://api.groq.com/openai/v1/chat/completions";
pub struct ChatEndpoint<'a> { pub url: String, pub key: Option<&'a str>, pub model: &'a str,
    pub groq_no_reasoning: bool }                                   // adds reasoning_effort "none"
pub fn chat_url(base_url: &str) -> String;                         // trims '/', appends "/chat/completions"
pub async fn polish_chat(client: &reqwest::Client, endpoint: ChatEndpoint<'_>, prompt: &PolishPrompt,
    input: &str) -> PolishOutcome;                                 // timeout + guards; never errors
pub struct LatestRelease { pub version: semver::Version, pub url: String }
pub fn latest_release_url(repository: &str) -> AppResult<String>; // github.com/o/n → releases/latest API
pub async fn latest_release(client: &reqwest::Client, url: &str, user_agent: &str)
    -> AppResult<LatestRelease>;                                   // tag "v0.3.0" → 0.3.0; 15 s timeout

// ── sv-audio ────────────────────────────────────────────────────────────────────────────
pub struct Microphone { pub id: String, pub name: String, pub is_default: bool, pub is_built_in: bool } // serde camelCase
pub fn list_microphones() -> AppResult<Vec<Microphone>>;
pub struct Recorder { .. }                                          // Debug; Send
impl Recorder {
    // Opens the device (None = built-in preferred, else system default) and starts capturing.
    // `on_level` gets 0..1 values ~30 times per second from a non-audio thread.
    pub fn start(device: Option<&str>, on_level: Box<dyn Fn(f32) + Send + 'static>) -> AppResult<Recorder>;
    pub fn stop(self) -> AppResult<Recording>;
}
pub struct Recording { pub samples: Vec<i16> /* 16 kHz mono */, pub duration_ms: i64 }
pub fn encode_wav(samples: &[i16]) -> Vec<u8>;                       // 16 kHz mono PCM16
pub fn decode_wav(bytes: &[u8]) -> AppResult<Vec<i16>>;              // for retry of kept audio
pub enum Cue { Start, Stop, Error }
pub struct CuePlayer { .. }                                          // Debug; owns rodio output on its own thread
impl CuePlayer {
    pub fn new() -> AppResult<CuePlayer>;
    pub fn play(&self, theme: SoundTheme, cue: Cue, volume: f32);    // never blocks; errors logged
}

// ── sv-engine ───────────────────────────────────────────────────────────────────────────
// The app spawns the sidecar (tauri-plugin-shell needs an AppHandle) and wires the pipes, so this
// crate has no Tauri dependency and is unit-testable with an in-memory transport.
pub trait Transport: Send + Sync + 'static { fn send_line(&self, line: String) -> AppResult<()>; }
#[derive(Clone, Debug)] pub struct EngineClient { .. }
impl EngineClient {
    pub fn new() -> EngineClient;                                    // disconnected
    pub fn connect(&self, transport: Arc<dyn Transport>);            // after (re)spawn
    pub fn on_stdout_line(&self, line: &str);                        // app forwards each stdout line
    pub fn on_terminated(&self);                                     // fails all pending, disconnects
    pub fn set_event_handler(&self, handler: EventHandler);          // id-less events; survives restarts
    pub async fn request<T: DeserializeOwned>(&self, cmd: &str, params: serde_json::Value,
        timeout: Duration) -> AppResult<T>;
    pub async fn request_with_progress<T: DeserializeOwned>(&self, cmd: &str, params: serde_json::Value,
        on_progress: Box<dyn Fn(f32, Option<String>) + Send + Sync>, timeout: Duration) -> AppResult<T>;
}
// protocol.rs: typed async methods on EngineClient (result structs re-exported at the crate root;
// `ProgressCallback` = the boxed progress closure above). `line` passed to `send_line` has no
// trailing newline; `on_stdout_line` trims and ignores blank, non-JSON, and unknown-id lines, and
// hands id-less lines to the event handler as `EngineEvent::FnKey { action: FnKeyAction }`
// (`Press | Release | Tap | Chord`); unknown events are ignored.
//   ping() -> PingResult { version }
//   model_status(model: &str, language: Option<&str>)
//       -> ModelStatusResult { status: ModelStatus, size_bytes: Option<u64>, reason: Option<String> }
//   download_model(model: &str, language: Option<&str>, on_progress: ProgressCallback) -> ()
//   delete_model(model: &str) -> ();  preload(model: &str) -> ()
//   transcribe(model: &str, wav_path: &Path, language: Option<&str>)
//       -> EngineTranscript { text, language: Option<String> }
//   permissions() -> Permissions;  request_permission(kind: PermissionKind) -> Permissions
//   open_settings(kind: PermissionKind) -> ()
//   frontmost_app() -> FrontmostApp { name: Option<String>, bundle_id: Option<String> }
//   paste() -> ()   // "accessibility permission missing" maps to AppError::Permission
//   set_output_muted(muted: bool) -> bool   // previous muted state
//   watch_fn_key(enabled: bool) -> bool   // whether the Fn key tap is installed now
//   polish(system: &str, shots: &[(String, String)], user: &str) -> PolishReply { text, finished }
// All return AppResult<_>; timeouts: download 2 h, preload 5 min, request_permission 10 min,
// transcribe 120 s, polish 30 s (callers apply their own tighter budget), others 10–30 s.
```

### UI (`src/`)

Vertical slices in `src/features/<feature>/` (`history/`, `insights/`, `dictionary/`, `style/`,
`settings/`, `onboarding/`); shared primitives in `src/ui/`; design tokens in
`src/styles/`; the command/event contract in `src/lib/api.ts`; shell (sidebar, routing) in
`src/app/`.

Shared UI state lives in two React contexts in `src/app/`: `SettingsContext.tsx`
(`useSettings()` → `{ settings, update(patch): Promise<boolean>, refresh() }`, kept current by
`settings-changed`; `update` shows its own error toast and resolves `false`) and
`ShellContext.tsx` (`useShell()` → `{ navigate(page) }`, with
`Page = "home" | "insights" | "dictionary" | "style" | SettingsSection` and
`SettingsSection = "general" | "system" | "models" | "permissions" | "data"`). Each settings
section is a first-class page: the sidebar lists it under a Settings group at the bottom and
`settings/SettingsPage.tsx` renders it in the content area like any other page. Tauri events are
consumed with `useTauriEvent(events.x, handler)` from `src/lib/useTauriEvent.ts`; toasts with
`useToast()` from `src/ui`.

Accessibility access: `settings/permissions/AccessibilityWarning.tsx` has no dismiss control. The
shell pins it (sticky) above every page whenever `permissions.accessibility !== "granted"`, saying
that text is only copied, and that Fn does nothing when `holdShortcut` or `toggleShortcut` is
`"Fn"`. The General settings and the onboarding shortcut step show it inline only while Fn is a
shortcut. The helper gates paste and its Fn event tap on the same grant, so the warning, paste and
the tap agree. `AXIsProcessTrusted()` can stay false inside a running process after the user
grants access, so while it does the helper re-checks in a short-lived copy of itself
(`simple-voice-engine --check-accessibility`, exit status 0 when trusted, reused for 1 s); the
2 s permission poller then emits `permissions-changed` and the warning disappears without a
relaunch.

Appearance: `settings.theme` (`"system" | "light" | "dark"`) is applied by the UI, not the
backend (`src/app/theme.ts`). The main window always carries the resolved theme as
`data-theme="light"|"dark"` on `<html>` (for `system`, resolved from
`prefers-color-scheme` and re-resolved live when macOS changes), and calls
`getCurrentWindow().setTheme(theme)`, or `setTheme(null)` for `system`, so the native title
bar matches. The last choice is cached in `localStorage["sv.theme"]` and painted before React
mounts, so launch never flashes the other theme. The overlay pill (§ 5) keeps its fixed palette.
`src-tauri/capabilities/default.json` grants `core:window:allow-set-theme` for this.

## 3. Tauri commands (invoked from the UI; TypeScript types in `src/lib/api.ts`)

All commands return `Result<T, AppError>`; the UI receives the error message string.

| Command                                                                        | Args                                                | Returns                                                                                                          |
| ------------------------------------------------------------------------------ | --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `get_settings`                                                                 | —                                                   | `Settings`                                                                                                       |
| `update_settings`                                                              | `patch: SettingsPatch`                              | `Settings` (re-registers shortcuts, dock, autostart, overlay as needed; invalid shortcut → error, nothing saved) |
| `set_groq_api_key`                                                             | `key: string \| null`                               | `Settings`                                                                                                       |
| `verify_groq_api_key`                                                          | `key: string`                                       | `null`                                                                                                           |
| `set_custom_api_key`                                                           | `key: string \| null`                               | `Settings`                                                                                                       |
| `test_post_processing`                                                         | —                                                   | `{ output: string, latencyMs: number }` (formats a fixed sample with the current provider)                       |
| `set_database_dir`                                                             | `dir: string`                                       | `Settings`                                                                                                       |
| `suspend_shortcuts`                                                            | `suspended: boolean`                                | `null` (UI suspends while recording a new shortcut)                                                              |
| `list_microphones`                                                             | —                                                   | `Microphone[]`                                                                                                   |
| `list_history`                                                                 | `query?: string, limit: number, beforeId?: number`  | `HistoryEntry[]`                                                                                                 |
| `delete_history`                                                               | `id`                                                | `null`                                                                                                           |
| `clear_history`                                                                | —                                                   | `null`                                                                                                           |
| `retry_history`                                                                | `id`                                                | `HistoryEntry`                                                                                                   |
| `copy_text`                                                                    | `text`                                              | `null`                                                                                                           |
| `list_dictionary`                                                              | —                                                   | `DictionaryEntry[]`                                                                                              |
| `add_dictionary_entry`                                                         | `phrase, replacement?`                              | `DictionaryEntry`                                                                                                |
| `update_dictionary_entry`                                                      | `id, phrase, replacement?`                          | `DictionaryEntry`                                                                                                |
| `delete_dictionary_entry`                                                      | `id`                                                | `null`                                                                                                           |
| `import_vocabulary`                                                            | `path: string`                                      | `ImportSummary`                                                                                                  |
| `get_insights`                                                                 | —                                                   | `Insights`                                                                                                       |
| `list_models`                                                                  | —                                                   | `ModelInfo[]` (transcription models + `apple-intelligence` post-processing availability)                         |
| `download_model`                                                               | `id`                                                | `null` (progress via events)                                                                                     |
| `delete_model`                                                                 | `id`                                                | `null`                                                                                                           |
| `get_permissions`                                                              | —                                                   | `Permissions`                                                                                                    |
| `request_permission`                                                           | `kind: "microphone" \| "accessibility" \| "speech"` | `Permissions`                                                                                                    |
| `open_permission_settings`                                                     | `kind`                                              | `null`                                                                                                           |
| `preview_sound`                                                                | `theme: string`                                     | `null`                                                                                                           |
| `start_dictation` / `stop_dictation` / `cancel_dictation` / `toggle_dictation` | —                                                   | `null`                                                                                                           |
| `app_info`                                                                     | —                                                   | `AppInfo`                                                                                                        |
| `get_update_status`                                                            | —                                                   | `UpdateStatus` (cached; no network)                                                                              |
| `check_for_updates`                                                            | —                                                   | `UpdateStatus` (checks now; a failure is reported in `error`, not as a command error)                            |
| `install_update`                                                               | —                                                   | `UpdateStatus` with `installing: true` (starts the install script; fails when no update is available)           |

## 4. Events (Rust → UI)

| Event                 | Payload                                                                                                                                           |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `dictation-state`     | `DictationState`                                                                                                                                  |
| `history-changed`     | `null`                                                                                                                                            |
| `settings-changed`    | `Settings`                                                                                                                                        |
| `model-progress`      | `{ id, fraction, status: "downloading" \| "ready" \| "failed", message? }`                                                                        |
| `permissions-changed` | `Permissions`                                                                                                                                     |
| `navigate`            | `"settings" \| "updates"` — sent to `main` by the tray / app menu; the main window shows Settings → General, or Settings → System for `"updates"` |
| `update-status`       | `UpdateStatus` — when a check starts and ends, and when an install starts and ends without quitting the app                                      |

`UpdateStatus`: `{ currentVersion: string, latest: { version: string, url: string } | null, updateAvailable: boolean, checking: boolean, checkedAt: number | null, error: string | null, installing: boolean, installError: string | null }`.

`DictationState`: `{ phase: "idle" | "recording" | "transcribing" | "formatting" | "done" | "error" | "cancelled", sessionId: number, startedAt?: number, message?: string, words?: number, note?: string }`.

## 5. Windows

- `main` — 1080×720 (min 860×600), `titleBarStyle: "Overlay"`, hidden title, traffic lights
  inset. Closing hides it; the tray reopens it. The Dock icon shows only while `main` is visible
  (and `Settings.showInDock` is on): hiding `main` switches the app to the Accessory activation
  policy, so with the window closed there is no Dock "Quit" to stop the shortcuts. The app menu
  binds Cmd+Q to "Close to Menu Bar" (hides `main`, like closing) and offers "Quit Simple Voice"
  without a shortcut; the tray's Quit, the Dock's Quit, logout and the update installer still quit
  the app.
- The overlay pill is not a Tauri window: the Swift helper draws it (`engine/.../Overlay.swift`,
  `PillView.swift`) because floating over full-screen apps needs the AppKit
  `fullScreenAuxiliary` collection behaviour, which Tauri does not expose and Rust cannot set
  without `unsafe`. It is a 200×56 borderless, non-activating `NSPanel` at status-bar level,
  joining all Spaces, click-through, never key, untitled (tiling window managers such as
  AeroSpace treat it as a popup). The helper is an accessory app, so ordering the pill in or out
  never activates Simple Voice or takes focus from the dictation target. `platform/overlay.rs`
  decides what it shows and when: visible while a session is active (or always when
  `showBarAlways`), hidden 1.2 s after `done` / `error` / `cancelled`, placed bottom-centre of
  the visible frame of the screen under the cursor, 80 pt up. It replays the last state and
  visibility to a restarted helper.

## 6. Engine helper protocol (`engine/`)

Launch: `simple-voice-engine --models-dir <path>`. One JSON object per line on stdin; one per
line on stdout. Logs go to stderr only.

Request: `{"id": 7, "cmd": "<name>", ...params}`
Result: `{"id": 7, "ok": true, "result": {...}}` or `{"id": 7, "ok": false, "error": "message"}`
Progress (zero or more before the result): `{"id": 7, "event": "progress", "fraction": 0.42, "message": "..."}`
Fn key (unsolicited, no id, only while `watch_fn_key` is enabled): `{"event": "fn_key", "action": A}`;
A = `"press"` (Fn held 100 ms with nothing else pressed), `"release"` (up after `press`), `"tap"`
(up before `press`), `"chord"` (another key or modifier joined after `press`, e.g. Fn+Arrow; no
`release` follows). Fn pressed while another modifier is held, or joined by a key within 100 ms,
produces nothing.

| cmd                  | Params                                                   | Result                                                                                                                                                       |
| -------------------- | -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `ping`               | —                                                        | `{"version": "x.y.z"}`                                                                                                                                       |
| `model_status`       | `model`, `language`?                                     | `{"status": "ready" \| "not_downloaded" \| "unsupported", "sizeBytes"?: n, "reason"?: s}`                                                                    |
| `download_model`     | `model`, `language`?                                     | `{"status": "ready"}` after progress events                                                                                                                  |
| `delete_model`       | `model`                                                  | `{}`                                                                                                                                                         |
| `preload`            | `model`                                                  | `{}` (loads into memory so the first dictation is fast)                                                                                                      |
| `transcribe`         | `model`, `wavPath` (16 kHz mono 16-bit PCM), `language`? | `{"text": s, "language"?: s}`                                                                                                                                |
| `permissions`        | —                                                        | `{"microphone": P, "accessibility": P, "speech": P}`; P = `"granted" \| "denied" \| "not_determined" \| "restricted"` (accessibility is only granted/denied) |
| `request_permission` | `kind`                                                   | same as `permissions`                                                                                                                                        |
| `open_settings`      | `kind`                                                   | `{}` (opens the matching Privacy & Security pane)                                                                                                            |
| `frontmost_app`      | —                                                        | `{"name": s?, "bundleId": s?}`                                                                                                                               |
| `paste`              | —                                                        | `{}` (posts Cmd+V; error `"accessibility permission missing"` when not trusted)                                                                              |
| `set_output_muted`   | `muted: bool`                                            | `{"previous": bool}`                                                                                                                                         |
| `polish`             | `system`, `shots: [{user, assistant}]`, `user`           | `{"text": s, "finished": bool}` via Apple Foundation Models (`LanguageModelSession`, temperature 0)                                                          |
| `watch_fn_key`       | `enabled: bool`                                          | `{"active": bool}`: whether the listen-only event tap is installed; without Accessibility access it retries every 2 s while enabled                          |

Notification (app → helper, no id, never answered, applied on the main thread in the order sent):
`{"cmd": "<name>", ...params}`. They drive the overlay pill:

| cmd               | Params                                                                                   |
| ----------------- | ---------------------------------------------------------------------------------------- |
| `overlay_state`   | `state: DictationState` (§ 4); done, error and cancelled settle back to idle after 1.1 s, 2 s and 0.35 s |
| `overlay_visible` | `visible: bool`; `true` (re)places the pill under the cursor and orders it in             |
| `overlay_level`   | `level: number` 0..1 (RMS), ~30 Hz while recording                                        |

Model ids: `parakeet-tdt-v3`, `parakeet-tdt-v2`, `parakeet-flash`, `apple-speech` (transcription);
`apple-intelligence` (post-processing; `model_status` reports availability, never downloads).
`groq-whisper` is cloud-only and never reaches the helper.
`language` (optional everywhere) is a language code such as `en` or `hi`, or a full locale such as
`en-GB`; only `apple-speech` uses it for `model_status` / `download_model` (its assets are per
locale; default `en-US`). Parakeet files live in `<models-dir>/<model-id>/`, downloaded into
`<model-id>.partial/` and renamed into place when complete.

## 7. Transcription × post-processing

Any transcription model combines with any post-processing provider:

|                                 | Groq LLM | Apple Intelligence (on-device) | Custom OpenAI-compatible (Ollama local or any remote) | Off             |
| ------------------------------- | -------- | ------------------------------ | ----------------------------------------------------- | --------------- |
| Groq Whisper (cloud)            | default  | ✓                              | ✓                                                     | ✓               |
| Parakeet / Apple Speech (local) | ✓        | ✓ fully offline                | ✓                                                     | ✓ fully offline |

The Groq API key is entered in Settings → Models, next to the Groq Whisper model and the Groq
post-processing provider; one key serves both.

## 8. Update notices

The install script (`scripts/install.sh`, P-5) installs every version. The `updates` slice asks
GitHub's `releases/latest` endpoint (drafts and pre-releases excluded, the same source the
install script follows) for the repository in the workspace `repository` field, 30 s
after launch and then whenever 24 hours of wall-clock time have passed since the last successful
check (it wakes hourly, since monotonic sleeps stop while the Mac sleeps). A failed check is
retried on the next wake. `Settings.checkForUpdates = false` stops the automatic checks; "Check
now", the tray item and the app-menu item still check on demand. When the release is newer than
the running version (semver), the tray item reads "Update Available: X…", and the UI shows a
banner above every page (hidden for `Settings.skippedUpdate`) and a row in Settings → System,
both with an Install update button.

`install_update` runs `curl -fsSL <repo>/releases/latest/download/install.sh | bash` in its own
process group with stdin closed and stdout and stderr in `~/.simplevoice/update.log`, so the
script survives the app quitting under it. The script downloads the release zip, verifies its
checksum and code signature, quits the app, replaces `~/Applications/Simple Voice.app` and reopens
it; it never needs an administrator password, removes a copy an older script left in
`/Applications` when the user may delete it, and reopens the old app when it fails after quitting
it. If the script exits while the app still runs, the slice clears `installing` and, on
failure, sets `installError` to the script's last `simple-voice:` line (else the last output
line) plus the log path.
