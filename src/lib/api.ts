// FilePath: src/lib/api.ts
// Typed wrappers for every Tauri command and event. This file is the UI half of the contract
// in docs/internal/architecture.md; keep both in sync.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Style = "formal" | "casual";
export type SoundTheme = "soft" | "glass" | "pop" | "chime";
export type Theme = "system" | "light" | "dark";
export type PostProcessing = "groq" | "apple" | "custom" | "off";
export type PermissionKind = "microphone" | "accessibility" | "speech";
export type PermissionStatus = "granted" | "denied" | "not_determined" | "restricted";

export interface Settings {
    holdShortcut: string;
    toggleShortcut: string;
    microphone: string | null;
    languages: string[];
    fallbackLanguage: string;
    transcriptionModel: string;
    style: Style;
    groqApiKeyPresent: boolean;
    sounds: boolean;
    soundTheme: SoundTheme;
    soundVolume: number;
    muteWhileDictating: boolean;
    launchAtLogin: boolean;
    showBarAlways: boolean;
    postProcessing: PostProcessing;
    groqFormattingModel: string;
    customBaseUrl: string;
    customModel: string;
    customApiKeyPresent: boolean;
    showInDock: boolean;
    theme: Theme;
    restoreClipboard: boolean;
    maxRecordingSeconds: number;
    onboardingComplete: boolean;
    checkForUpdates: boolean;
    /** Release version whose banner the user dismissed; empty = none. */
    skippedUpdate: string;
    databaseDir: string;
}

export type SettingsPatch = Partial<
    Omit<Settings, "groqApiKeyPresent" | "customApiKeyPresent" | "databaseDir">
>;

export interface Microphone {
    id: string;
    name: string;
    isDefault: boolean;
    isBuiltIn: boolean;
}

export type HistoryStatus = "pasted" | "unformatted" | "failed" | "dropped" | "not_pasted";

export type AppCategory =
    "work_messages" | "personal_messages" | "email" | "documents" | "ai_prompts" | "code" | "other";

export interface HistoryEntry {
    id: number;
    createdAt: number;
    status: HistoryStatus;
    rawText: string;
    text: string;
    error: string | null;
    model: string;
    language: string | null;
    style: Style;
    audioMs: number;
    latencyMs: number;
    wordCount: number;
    dictionaryFixes: number;
    wordsCorrected: number;
    appName: string | null;
    bundleId: string | null;
    appCategory: AppCategory;
    canRetry: boolean;
}

export interface DictionaryEntry {
    id: number;
    phrase: string;
    replacement: string | null;
    createdAt: number;
}

export interface ImportSummary {
    added: number;
    skipped: number;
}

export interface DayActivity {
    /** Local calendar date, `YYYY-MM-DD`. */
    date: string;
    words: number;
    dictations: number;
}

export interface CategoryUsage {
    category: AppCategory;
    dictations: number;
    words: number;
    /** 0..100, share of dictations. */
    percent: number;
}

export interface AppUsage {
    name: string;
    bundleId: string | null;
    words: number;
}

export interface Insights {
    totalWords: number;
    totalDictations: number;
    /** Words per minute of speaking (words / audio minutes). 0 when no data. */
    wordsPerMinute: number;
    /** Minutes saved versus typing at 40 wpm. */
    timeSavedMinutes: number;
    dictionaryFixes: number;
    wordsCorrected: number;
    currentStreak: number;
    longestStreak: number;
    wordsThisMonth: number;
    /** Percent change of words this month vs last month; null when last month had none. */
    monthChangePercent: number | null;
    /** Last 26 weeks, oldest first, one entry per day including zero days. */
    days: DayActivity[];
    categories: CategoryUsage[];
    topApps: AppUsage[];
}

export type ModelKind = "transcription" | "post_processing";
export type ModelProvider = "groq" | "parakeet" | "apple" | "custom";
export type ModelStatus = "cloud" | "ready" | "not_downloaded" | "downloading" | "unsupported";

export interface ModelInfo {
    id: string;
    kind: ModelKind;
    provider: ModelProvider;
    name: string;
    subtitle: string;
    /** 0..100 */
    speed: number;
    /** 0..100 */
    accuracy: number;
    languages: string;
    sizeMb: number | null;
    status: ModelStatus;
    reason: string | null;
    progress: number | null;
}

export interface Permissions {
    microphone: PermissionStatus;
    accessibility: PermissionStatus;
    speech: PermissionStatus;
}

export interface AppInfo {
    version: string;
    dataDir: string;
    databasePath: string;
    modelsDir: string;
    engineVersion: string | null;
}

export interface Release {
    version: string;
    url: string;
}

export interface UpdateStatus {
    currentVersion: string;
    latest: Release | null;
    updateAvailable: boolean;
    checking: boolean;
    checkedAt: number | null;
    error: string | null;
    /** The install script is running; a successful install quits and reopens the app. */
    installing: boolean;
    /** Why the last install failed; cleared when the next one starts. */
    installError: string | null;
}

export type DictationPhase =
    "idle" | "recording" | "transcribing" | "formatting" | "done" | "error" | "cancelled";

export interface DictationState {
    phase: DictationPhase;
    sessionId: number;
    startedAt?: number;
    message?: string;
    words?: number;
    note?: string;
}

export interface ModelProgress {
    id: string;
    fraction: number;
    status: "downloading" | "ready" | "failed";
    message?: string;
}

export interface PostProcessingTest {
    output: string;
    latencyMs: number;
}

export type NavigateTarget = "settings" | "updates";

export const api = {
    getSettings: () => invoke<Settings>("get_settings"),
    updateSettings: (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch }),
    setGroqApiKey: (key: string | null) => invoke<Settings>("set_groq_api_key", { key }),
    verifyGroqApiKey: (key: string) => invoke<null>("verify_groq_api_key", { key }),
    setCustomApiKey: (key: string | null) => invoke<Settings>("set_custom_api_key", { key }),
    testPostProcessing: () => invoke<PostProcessingTest>("test_post_processing"),
    setDatabaseDir: (dir: string) => invoke<Settings>("set_database_dir", { dir }),
    suspendShortcuts: (suspended: boolean) => invoke<null>("suspend_shortcuts", { suspended }),
    listMicrophones: () => invoke<Microphone[]>("list_microphones"),
    listHistory: (limit: number, query?: string, beforeId?: number) =>
        invoke<HistoryEntry[]>("list_history", { query, limit, beforeId }),
    deleteHistory: (id: number) => invoke<null>("delete_history", { id }),
    clearHistory: () => invoke<null>("clear_history"),
    retryHistory: (id: number) => invoke<HistoryEntry>("retry_history", { id }),
    copyText: (text: string) => invoke<null>("copy_text", { text }),
    listDictionary: () => invoke<DictionaryEntry[]>("list_dictionary"),
    addDictionaryEntry: (phrase: string, replacement: string | null) =>
        invoke<DictionaryEntry>("add_dictionary_entry", { phrase, replacement }),
    updateDictionaryEntry: (id: number, phrase: string, replacement: string | null) =>
        invoke<DictionaryEntry>("update_dictionary_entry", { id, phrase, replacement }),
    deleteDictionaryEntry: (id: number) => invoke<null>("delete_dictionary_entry", { id }),
    importVocabulary: (path: string) => invoke<ImportSummary>("import_vocabulary", { path }),
    getInsights: () => invoke<Insights>("get_insights"),
    listModels: () => invoke<ModelInfo[]>("list_models"),
    downloadModel: (id: string) => invoke<null>("download_model", { id }),
    deleteModel: (id: string) => invoke<null>("delete_model", { id }),
    getPermissions: () => invoke<Permissions>("get_permissions"),
    requestPermission: (kind: PermissionKind) =>
        invoke<Permissions>("request_permission", { kind }),
    openPermissionSettings: (kind: PermissionKind) =>
        invoke<null>("open_permission_settings", { kind }),
    previewSound: (theme: SoundTheme) => invoke<null>("preview_sound", { theme }),
    startDictation: () => invoke<null>("start_dictation"),
    stopDictation: () => invoke<null>("stop_dictation"),
    cancelDictation: () => invoke<null>("cancel_dictation"),
    toggleDictation: () => invoke<null>("toggle_dictation"),
    appInfo: () => invoke<AppInfo>("app_info"),
    getUpdateStatus: () => invoke<UpdateStatus>("get_update_status"),
    checkForUpdates: () => invoke<UpdateStatus>("check_for_updates"),
    installUpdate: () => invoke<UpdateStatus>("install_update"),
};

export const events = {
    dictationState: (cb: (state: DictationState) => void): Promise<UnlistenFn> =>
        listen<DictationState>("dictation-state", (e) => {
            cb(e.payload);
        }),
    dictationLevel: (cb: (level: number) => void): Promise<UnlistenFn> =>
        listen<{ level: number }>("dictation-level", (e) => {
            cb(e.payload.level);
        }),
    historyChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("history-changed", () => {
            cb();
        }),
    settingsChanged: (cb: (settings: Settings) => void): Promise<UnlistenFn> =>
        listen<Settings>("settings-changed", (e) => {
            cb(e.payload);
        }),
    modelProgress: (cb: (progress: ModelProgress) => void): Promise<UnlistenFn> =>
        listen<ModelProgress>("model-progress", (e) => {
            cb(e.payload);
        }),
    permissionsChanged: (cb: (permissions: Permissions) => void): Promise<UnlistenFn> =>
        listen<Permissions>("permissions-changed", (e) => {
            cb(e.payload);
        }),
    navigate: (cb: (target: NavigateTarget) => void): Promise<UnlistenFn> =>
        listen<NavigateTarget>("navigate", (e) => {
            cb(e.payload);
        }),
    updateStatus: (cb: (status: UpdateStatus) => void): Promise<UnlistenFn> =>
        listen<UpdateStatus>("update-status", (e) => {
            cb(e.payload);
        }),
};

/** Error message from a rejected command (Rust `AppError` serializes to a string). */
export function errorMessage(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
}
