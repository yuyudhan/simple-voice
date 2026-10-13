// FilePath: src/lib/api/meetings.ts
// Wire types of personal meetings: sessions, speakers, timestamped segments, summaries, chat,
// translations, readiness and progress events.

export type MeetingStatus = "recording" | "processing" | "ready" | "failed" | "cancelled";

export interface Meeting {
    id: number;
    title: string;
    startedAt: number;
    endedAt: number | null;
    durationMs: number;
    status: MeetingStatus;
    microphone: boolean;
    systemAudio: boolean;
    language: string | null;
    error: string | null;
    segmentCount: number;
    speakerCount: number;
}

/** A voice the diarization model told apart; `name` is what the user called it. */
export interface Speaker {
    id: number;
    /** Assigned by diarization, e.g. "Speaker 1". */
    label: string;
    name: string | null;
}

export interface Segment {
    id: number;
    startMs: number;
    endMs: number;
    speakerId: number | null;
    text: string;
}

export interface ActionItem {
    text: string;
    owner: string | null;
    startMs: number | null;
}

export interface MeetingSummary {
    text: string;
    actionItems: ActionItem[];
    model: string;
    createdAt: number;
}

export interface Citation {
    segmentId: number;
    startMs: number;
}

export interface ChatMessage {
    id: number;
    role: "user" | "assistant";
    text: string;
    citations: Citation[];
    createdAt: number;
}

export interface Translation {
    language: string;
    model: string;
    createdAt: number;
    segments: { segmentId: number; text: string }[];
}

export interface MeetingDetail {
    meeting: Meeting;
    speakers: Speaker[];
    segments: Segment[];
    summary: MeetingSummary | null;
    chat: ChatMessage[];
    translations: Translation[];
}

/** Why each meeting stage cannot run yet; null = its local model is ready. */
export interface MeetingReadiness {
    transcription: string | null;
    diarization: string | null;
    /** Summary and chat: the meeting assistant model. */
    language: string | null;
    /** Translation: the dedicated translation model, independent of the assistant. */
    translation: string | null;
}

export interface MeetingState {
    id: number;
    status: MeetingStatus;
    elapsedMs: number;
    message?: string;
}

export type MeetingJob = "transcribe" | "diarize" | "summarize" | "chat" | "translate";

export interface MeetingProgress {
    id: number;
    job: MeetingJob;
    /** 0..1 when measurable. */
    fraction: number | null;
    message: string | null;
}

export type MeetingExportFormat = "markdown" | "text" | "json";
