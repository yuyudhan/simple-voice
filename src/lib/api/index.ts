// FilePath: src/lib/api/index.ts
// Typed wrappers for every Tauri command and event. This module is the UI half of the contract
// in docs/internal/architecture.md; keep both in sync. The wire types live in ./types and
// ./meetings and are exported from here, so callers import everything from "lib/api".
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
    SoundTheme,
    PermissionKind,
    Settings,
    SettingsPatch,
    KnownApp,
    Microphone,
    HistoryEntry,
    DictionaryEntry,
    Snippet,
    ImportSummary,
    Insights,
    ModelInsights,
    ModelInfo,
    SmartSelectPlan,
    LanguageInfo,
    VocabularyPackInfo,
    Permissions,
    AppInfo,
    UpdateStatus,
    DictationState,
    ModelProgress,
    PostProcessingTest,
    DictationPartial,
    NavigateTarget,
} from "./types";
import type {
    Meeting,
    Speaker,
    Segment,
    MeetingSummary,
    ChatMessage,
    Translation,
    MeetingDetail,
    MeetingReadiness,
    MeetingState,
    MeetingProgress,
    MeetingExportFormat,
} from "./meetings";

export * from "./types";
export * from "./meetings";

export const api = {
    getSettings: () => invoke<Settings>("get_settings"),
    updateSettings: (patch: SettingsPatch) => invoke<Settings>("update_settings", { patch }),
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
    listVocabularyPacks: () => invoke<VocabularyPackInfo[]>("list_vocabulary_packs"),
    addDictionaryEntry: (phrase: string, replacement: string | null) =>
        invoke<DictionaryEntry>("add_dictionary_entry", { phrase, replacement }),
    updateDictionaryEntry: (id: number, phrase: string, replacement: string | null) =>
        invoke<DictionaryEntry>("update_dictionary_entry", { id, phrase, replacement }),
    deleteDictionaryEntry: (id: number) => invoke<null>("delete_dictionary_entry", { id }),
    importVocabulary: (path: string) => invoke<ImportSummary>("import_vocabulary", { path }),
    exportVocabulary: (path: string) => invoke<null>("export_vocabulary", { path }),
    listSnippets: () => invoke<Snippet[]>("list_snippets"),
    addSnippet: (trigger: string, text: string) =>
        invoke<Snippet>("add_snippet", { trigger, text }),
    updateSnippet: (id: number, trigger: string, text: string) =>
        invoke<Snippet>("update_snippet", { id, trigger, text }),
    deleteSnippet: (id: number) => invoke<null>("delete_snippet", { id }),
    listKnownApps: () => invoke<KnownApp[]>("list_known_apps"),
    listMeetings: () => invoke<Meeting[]>("list_meetings"),
    getMeeting: (id: number) => invoke<MeetingDetail>("get_meeting", { id }),
    meetingReadiness: () => invoke<MeetingReadiness>("meeting_readiness"),
    startMeeting: (title: string, systemAudio: boolean) =>
        invoke<Meeting>("start_meeting", { title, systemAudio }),
    /** Ends the recording; the returned meeting is processing. */
    stopMeeting: () => invoke<Meeting>("stop_meeting"),
    cancelMeeting: () => invoke<null>("cancel_meeting"),
    deleteMeeting: (id: number) => invoke<null>("delete_meeting", { id }),
    renameMeeting: (id: number, title: string) => invoke<Meeting>("rename_meeting", { id, title }),
    renameSpeaker: (meetingId: number, speakerId: number, name: string | null) =>
        invoke<Speaker>("rename_speaker", { meetingId, speakerId, name }),
    updateSegment: (id: number, text: string) => invoke<Segment>("update_segment", { id, text }),
    summarizeMeeting: (id: number) => invoke<MeetingSummary>("summarize_meeting", { id }),
    askMeeting: (id: number, question: string) =>
        invoke<ChatMessage>("ask_meeting", { id, question }),
    translateMeeting: (id: number, language: string) =>
        invoke<Translation>("translate_meeting", { id, language }),
    cancelMeetingJob: (id: number) => invoke<null>("cancel_meeting_job", { id }),
    /** Re-runs processing of a failed or cancelled meeting from its kept audio. */
    retryMeeting: (id: number) => invoke<Meeting>("retry_meeting", { id }),
    exportMeeting: (id: number, path: string, format: MeetingExportFormat) =>
        invoke<null>("export_meeting", { id, path, format }),
    getInsights: () => invoke<Insights>("get_insights"),
    getModelInsights: () => invoke<ModelInsights>("get_model_insights"),
    listModels: () => invoke<ModelInfo[]>("list_models"),
    downloadModel: (id: string) => invoke<null>("download_model", { id }),
    deleteModel: (id: string) => invoke<null>("delete_model", { id }),
    smartSelectPlan: () => invoke<SmartSelectPlan>("smart_select_plan"),
    listLanguages: () => invoke<LanguageInfo[]>("list_languages"),
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
    dismissUpdateNotice: () => invoke<UpdateStatus>("dismiss_update_notice"),
};

export const events = {
    dictationState: (cb: (state: DictationState) => void): Promise<UnlistenFn> =>
        listen<DictationState>("dictation-state", (e) => {
            cb(e.payload);
        }),
    historyChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("history-changed", () => {
            cb();
        }),
    dictionaryChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("dictionary-changed", () => {
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
    /** A model download completed or a delete succeeded. */
    modelsChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("models-changed", () => {
            cb();
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
    dictationPartial: (cb: (partial: DictationPartial) => void): Promise<UnlistenFn> =>
        listen<DictationPartial>("dictation-partial", (e) => {
            cb(e.payload);
        }),
    snippetsChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("snippets-changed", () => {
            cb();
        }),
    meetingsChanged: (cb: () => void): Promise<UnlistenFn> =>
        listen<null>("meetings-changed", () => {
            cb();
        }),
    meetingState: (cb: (state: MeetingState) => void): Promise<UnlistenFn> =>
        listen<MeetingState>("meeting-state", (e) => {
            cb(e.payload);
        }),
    meetingProgress: (cb: (progress: MeetingProgress) => void): Promise<UnlistenFn> =>
        listen<MeetingProgress>("meeting-progress", (e) => {
            cb(e.payload);
        }),
};

/** Error message from a rejected command (Rust `AppError` serializes to a string). */
export function errorMessage(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return String(error);
}
