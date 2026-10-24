// FilePath: src/lib/api/types.ts
// Wire types of the app's settings, dictation, history, dictionary, snippets, insights, models,
// permissions and updates. Field names and enums mirror the Rust side (camelCase structs,
// snake_case enums); docs/internal/architecture.md describes the contract.

export type Style = "formal" | "casual";
export type SoundTheme = "soft" | "glass" | "pop" | "chime";
export type Theme = "system" | "light" | "dark";
/** "apple" = Apple Intelligence on this Mac; "local" = the downloaded model `localLlmModel`. */
export type PostProcessing = "apple" | "local" | "off";
/** The on-device model behind meeting summaries, chat and translation. */
export type MeetingAssistant = "apple" | "local";
export type DictionarySort = "name_asc" | "name_desc" | "newest" | "oldest";
export type VocabularyPack =
    | "everyday"
    | "programming"
    | "web"
    | "backend"
    | "mobile"
    | "cloud"
    | "data_ai"
    | "sales"
    | "marketing"
    | "business";
export type PermissionKind = "microphone" | "accessibility" | "speech" | "system_audio";
export type PermissionStatus = "granted" | "denied" | "not_determined" | "restricted";

export interface Settings {
    holdShortcut: string;
    toggleShortcut: string;
    /** Toggle to edit the selected text by voice; empty = edit mode off. Never "Fn". */
    editShortcut: string;
    /** Esc cancels a recording in progress; off by default. */
    escapeCancels: boolean;
    microphone: string | null;
    languages: string[];
    fallbackLanguage: string;
    /** A model id, or "smart-select" to let the app pick per dictation. */
    transcriptionModel: string;
    /** Global writing style; category and app overrides win over it (app > category > global). */
    style: Style;
    /** Style per app category; a missing category uses the global `style`. */
    categoryStyles: Partial<Record<AppCategory, Style>>;
    /** Style for one app by bundle ID; wins over its category. */
    appStyles: AppStyleOverride[];
    sounds: boolean;
    soundTheme: SoundTheme;
    soundVolume: number;
    muteWhileDictating: boolean;
    launchAtLogin: boolean;
    showBarAlways: boolean;
    /** Seconds the pill shows the start of the pasted text after a dictation (1–10). */
    pastedTextSeconds: number;
    postProcessing: PostProcessing;
    /** Catalog id of the downloadable local language model (`postProcessing: "local"`). */
    localLlmModel: string;
    meetingAssistant: MeetingAssistant;
    /** Read the focused field's surrounding text (Accessibility) when a dictation starts. */
    foregroundContext: boolean;
    /** Read visible file and identifier names from a supported IDE when a dictation starts. */
    ideContext: boolean;
    /** Bundle IDs whose text is never read for context. */
    contextExcludedApps: string[];
    /** Show the live transcript while recording, when the voice model streams. */
    liveTranscript: boolean;
    /** Last choice of also recording system audio in a meeting. */
    meetingSystemAudio: boolean;
    /** BCP 47 tag last used to translate a meeting. */
    translationLanguage: string;
    showInDock: boolean;
    theme: Theme;
    restoreClipboard: boolean;
    maxRecordingSeconds: number;
    onboardingComplete: boolean;
    checkForUpdates: boolean;
    /** Release version whose banner the user dismissed; empty = none. */
    skippedUpdate: string;
    dictionarySort: DictionarySort;
    /** Learn dictionary words from corrections made to pasted text; needs post-processing. */
    learnFromEdits: boolean;
    /** Smart Select re-runs a dictation whose result looks unclear on the route's other model. */
    smartRetry: boolean;
    /** Built-in vocabulary packs in use, deduplicated, in the registry's order. */
    vocabularyPacks: VocabularyPack[];
    databaseDir: string;
}

export type SettingsPatch = Partial<Omit<Settings, "databaseDir">>;

export interface AppStyleOverride {
    bundleId: string;
    appName: string;
    style: Style;
}

/** An app the style override picker can offer: seen in history or frontmost now. */
export interface KnownApp {
    bundleId: string;
    appName: string;
    category: AppCategory;
}

export interface Microphone {
    id: string;
    name: string;
    isDefault: boolean;
    isBuiltIn: boolean;
}

export type HistoryStatus = "pasted" | "unformatted" | "failed" | "dropped" | "not_pasted";

/** Why Smart Select retried; "mixed_script" = the model wrote a romanised language (e.g.
 * Hinglish) partly in that language's own script; "wrong_script" = the text was not in the
 * chosen script (Devanagari for Hinglish, or no Devanagari at all for Hindi). */
export type RetryReason = "failed" | "low_confidence" | "mixed_script" | "wrong_script";

export type AppCategory =
    "work_messages" | "personal_messages" | "email" | "documents" | "ai_prompts" | "code" | "other";

export type StyleSource = "app" | "category" | "global";

export interface HistoryEntry {
    id: number;
    createdAt: number;
    status: HistoryStatus;
    rawText: string;
    text: string;
    /** The selected text an edit replaced; null for a dictation. */
    sourceText: string | null;
    error: string | null;
    model: string;
    /** The model that produced `rawText`. */
    modelName: string;
    /** Set only when Smart Select retried: the model tried first. */
    firstModelName: string | null;
    /** Why the first model's result was retried; set exactly when `firstModelName` is. */
    retryReason: RetryReason | null;
    /** Set only when a formatting pass produced the pasted text. */
    formatModelName: string | null;
    language: string | null;
    /** The style actually applied to this dictation. */
    style: Style;
    /** Which setting chose `style`; null for dictations recorded before style overrides. */
    styleSource: StyleSource | null;
    audioMs: number;
    latencyMs: number;
    /** How long the transcription model took; null when it failed or was never measured. */
    transcribeMs: number | null;
    /** How long the formatting pass took; set exactly when `formatModelName` is. */
    formatMs: number | null;
    wordCount: number;
    dictionaryFixes: number;
    wordsCorrected: number;
    appName: string | null;
    bundleId: string | null;
    appCategory: AppCategory;
    canRetry: boolean;
    /** What the pasted text read after the user corrected it; null when never corrected. */
    editedText: string | null;
}

export type DictionarySource = "manual" | "learned";

export interface DictionaryEntry {
    id: number;
    phrase: string;
    replacement: string | null;
    source: DictionarySource;
    createdAt: number;
}

/** A spoken trigger that expands to its exact text, whitespace and line breaks kept. */
export interface Snippet {
    id: number;
    trigger: string;
    text: string;
    createdAt: number;
    updatedAt: number;
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

export interface HourActivity {
    /** 0..23, local time. */
    hour: number;
    words: number;
    dictations: number;
}

/** All-time records; on a tie the earlier day holds the record. */
export interface PersonalBests {
    longestDictationWords: number;
    bestDayWords: number;
    /** `YYYY-MM-DD`, null without history. */
    bestDayDate: string | null;
    busiestDayDictations: number;
    busiestDayDate: string | null;
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
    /** 24 entries, hour 0 first, zero hours included. */
    hours: HourActivity[];
    bests: PersonalBests;
}

/** One model's speed over every dictation where its stage was timed. */
export interface ModelTiming {
    model: string;
    name: string;
    runs: number;
    averageMs: number;
    fastestMs: number;
    slowestMs: number;
    /** Sums over the timed runs, so speed is total work over total time. */
    totalMs: number;
    audioMs: number;
    words: number;
    /** Dictations where this model ran first and Smart Select retried on another model. */
    retries: number;
}

/** Each list sorted by runs, descending. */
export interface ModelInsights {
    transcription: ModelTiming[];
    formatting: ModelTiming[];
}

/** "post_processing" = a language model: Apple Intelligence or a downloadable local one;
 * "translation" = the dedicated meeting translation model; "vocabulary" = Parakeet's dictionary
 * booster. */
export type ModelKind =
    "transcription" | "post_processing" | "diarization" | "translation" | "vocabulary";
export type ModelProvider = "parakeet" | "whisper" | "apple" | "llm" | "diarizer" | "translator";
export type ModelStatus = "ready" | "not_downloaded" | "downloading" | "unsupported";

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
    /** Licence of the weights, e.g. "CC-BY-4.0"; null for models built into macOS. */
    license: string | null;
    /** Where the one-time download comes from; null for models built into macOS. */
    source: string | null;
    /** Recognises speech incrementally while recording (live transcript). */
    streaming: boolean;
}

/** What Smart Select runs for the current languages; always a model on this Mac. */
export interface SmartSelectPlan {
    /** Model id a dictation runs now: the best downloaded model for the languages. */
    local: string;
    /** Model id best for the languages, downloaded or not. */
    recommended: string;
    /** `local` is downloaded, so dictation can run. */
    ready: boolean;
    /** Set only when the local model can't write a selected language as chosen. */
    notice: string | null;
}

/** One entry of the language registry, as the language picker offers it. */
export interface LanguageInfo {
    /** BCP 47 tag stored in settings: "en", "hi", "hi-Latn". */
    tag: string;
    name: string;
    /** How it is written ("Roman script", "Devanagari"), only when another entry shares its base
     * language and the script is what tells them apart. */
    script: string | null;
    /** Offered as a one-click chip. */
    primary: boolean;
    /** May be the fallback language. */
    fallback: boolean;
}

/** One built-in vocabulary pack, as the Dictionary page offers it. */
export interface VocabularyPackInfo {
    id: VocabularyPack;
    name: string;
    description: string;
    /** Every spelling the pack teaches, in its priority order. */
    terms: string[];
}

export interface Permissions {
    microphone: PermissionStatus;
    accessibility: PermissionStatus;
    speech: PermissionStatus;
    /** Screen & System Audio Recording, needed only to record other apps' audio in a meeting. */
    systemAudio: PermissionStatus;
}

/** Where each permission's status lives in `Permissions`. */
export const PERMISSION_FIELD: Record<PermissionKind, keyof Permissions> = {
    microphone: "microphone",
    accessibility: "accessibility",
    speech: "speech",
    system_audio: "systemAudio",
};

export interface AppInfo {
    version: string;
    dataDir: string;
    databasePath: string;
    modelsDir: string;
    engineVersion: string | null;
    /** Debug build (`just dev`); the sidebar labels it apart from the installed app. */
    dev: boolean;
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
    /** A newer version is already on disk under the running app; a restart finishes the update. */
    installedVersion: string | null;
    /** The older version that ran before this launch: an update just finished. */
    updatedFrom: string | null;
}

export type DictationPhase =
    "idle" | "recording" | "transcribing" | "formatting" | "done" | "error" | "cancelled";

export interface DictationState {
    phase: DictationPhase;
    sessionId: number;
    startedAt?: number;
    message?: string;
    /** Start of the pasted text, one line, at most 80 characters then "..." (done only). */
    text?: string;
    note?: string;
    /** The session edits the selected text (edit mode); absent for a dictation. */
    edit?: boolean;
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

/** Live recognition while recording: `text` is final and never changes, `tentative` may. */
export interface DictationPartial {
    sessionId: number;
    text: string;
    tentative: string;
}

export type NavigateTarget = "settings" | "updates";
