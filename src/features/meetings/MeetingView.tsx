// FilePath: src/features/meetings/MeetingView.tsx
// One meeting: its title, sources and state, the processing progress, and the transcript,
// summary, questions and translation as tabs. Export writes a file the user picks; delete
// removes the meeting and its audio from this Mac.
import { useCallback, useEffect, useState } from "react";
import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { Download, Mic, MonitorSpeaker, RotateCcw, Trash2 } from "lucide-react";
import {
    api,
    errorMessage,
    events,
    type MeetingDetail,
    type MeetingExportFormat,
    type MeetingJob,
    type MeetingProgress,
    type MeetingReadiness,
} from "../../lib/api";
import { formatLongDate, formatTime } from "../../lib/format";
import { useTauriEvent } from "../../lib/useTauriEvent";
import {
    Button,
    Modal,
    ProgressBar,
    Segmented,
    Spinner,
    Tabs,
    TextField,
    useToast,
    type SegmentedOption,
} from "../../ui";
import { AskView } from "./AskView";
import { MeetingStatusBadge } from "./MeetingStatusBadge";
import { SummaryView } from "./SummaryView";
import { TranscriptView } from "./TranscriptView";
import { TranslateView } from "./TranslateView";
import { formatClock } from "./useMeetings";

type Tab = "transcript" | "summary" | "ask" | "translate";

const TABS: { id: Tab; label: string }[] = [
    { id: "transcript", label: "Transcript" },
    { id: "summary", label: "Summary" },
    { id: "ask", label: "Ask" },
    { id: "translate", label: "Translate" },
];

const EXPORTS: Record<MeetingExportFormat, { label: string; extension: string }> = {
    markdown: { label: "Markdown", extension: "md" },
    text: { label: "Plain text", extension: "txt" },
    json: { label: "JSON", extension: "json" },
};

const EXPORT_OPTIONS: SegmentedOption<MeetingExportFormat>[] = [
    { value: "markdown", label: EXPORTS.markdown.label },
    { value: "text", label: EXPORTS.text.label },
    { value: "json", label: EXPORTS.json.label },
];

const PROCESSING_LABEL: Partial<Record<MeetingJob, string>> = {
    transcribe: "Transcribing on this Mac",
    diarize: "Telling speakers apart",
};

/** Safe default file name from a meeting title. */
function fileStem(title: string): string {
    const stem = title.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-+|-+$/g, "");
    return stem === "" ? "meeting" : stem;
}

interface TitleProps {
    meetingId: number;
    title: string;
    onRenamed: () => void;
}

function TitleField({ meetingId, title, onRenamed }: TitleProps) {
    const { toast } = useToast();
    const [draft, setDraft] = useState(title);

    const commit = async () => {
        const next = draft.trim();
        if (next === title) return;
        if (next === "") {
            setDraft(title);
            return;
        }
        try {
            await api.renameMeeting(meetingId, next);
            onRenamed();
        } catch (error) {
            setDraft(title);
            toast(errorMessage(error), "danger");
        }
    };

    return (
        <TextField
            className="meeting-view__title"
            aria-label="Meeting title"
            value={draft}
            onChange={(event) => {
                setDraft(event.target.value);
            }}
            onBlur={() => {
                void commit();
            }}
            onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
                if (event.key === "Escape") {
                    setDraft(title);
                    event.currentTarget.blur();
                }
            }}
        />
    );
}

function ExportDialog({ detail, onClose }: { detail: MeetingDetail; onClose: () => void }) {
    const { toast } = useToast();
    const [format, setFormat] = useState<MeetingExportFormat>("markdown");
    const [exporting, setExporting] = useState(false);

    async function run() {
        const { extension, label } = EXPORTS[format];
        try {
            const path = await saveFileDialog({
                defaultPath: `${fileStem(detail.meeting.title)}.${extension}`,
                filters: [{ name: label, extensions: [extension] }],
            });
            if (path === null) return;
            setExporting(true);
            await api.exportMeeting(detail.meeting.id, path, format);
            toast("Meeting exported", "success");
            onClose();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setExporting(false);
        }
    }

    return (
        <Modal
            open
            onClose={onClose}
            title="Export meeting"
            width={440}
            footer={
                <>
                    <Button variant="ghost" onClick={onClose} disabled={exporting}>
                        Cancel
                    </Button>
                    <Button
                        variant="primary"
                        loading={exporting}
                        onClick={() => {
                            void run();
                        }}
                    >
                        Export…
                    </Button>
                </>
            }
        >
            <div className="meeting-export">
                <Segmented
                    label="Export format"
                    value={format}
                    options={EXPORT_OPTIONS}
                    onChange={setFormat}
                />
                <p className="sv-inline-note">
                    The transcript with times and speaker names, plus the summary, action items,
                    questions and translations. The file is written only where you choose.
                </p>
            </div>
        </Modal>
    );
}

interface Props {
    meetingId: number;
    readiness: MeetingReadiness | null;
    onDeleted: () => void;
}

export function MeetingView({ meetingId, readiness, onDeleted }: Props) {
    const { toast } = useToast();
    const [detail, setDetail] = useState<MeetingDetail | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [tab, setTab] = useState<Tab>("transcript");
    const [focusMs, setFocusMs] = useState<number | null>(null);
    const [processing, setProcessing] = useState<Partial<Record<MeetingJob, MeetingProgress>>>({});
    const [exportOpen, setExportOpen] = useState(false);
    const [confirmDelete, setConfirmDelete] = useState(false);
    const [deleting, setDeleting] = useState(false);
    const [busy, setBusy] = useState<"retry" | "cancel" | null>(null);

    const load = useCallback(
        () =>
            api.getMeeting(meetingId).then(
                (next) => {
                    setDetail(next);
                    setLoadError(null);
                    if (next.meeting.status !== "processing") setProcessing({});
                },
                (error: unknown) => {
                    setLoadError(errorMessage(error));
                },
            ),
        [meetingId],
    );

    useEffect(() => {
        void load();
    }, [load]);

    useTauriEvent(events.meetingsChanged, () => {
        void load();
    });
    useTauriEvent(events.meetingState, (state) => {
        if (state.id === meetingId && state.status !== detail?.meeting.status) void load();
    });
    useTauriEvent(events.meetingProgress, (progress) => {
        if (progress.id !== meetingId || PROCESSING_LABEL[progress.job] === undefined) return;
        setProcessing((current) => ({ ...current, [progress.job]: progress }));
    });

    if (detail === null) {
        return (
            <div className="meeting-loading">
                {loadError ? (
                    <p className="sv-inline-error" role="alert">
                        {loadError}
                    </p>
                ) : (
                    <Spinner size={20} />
                )}
            </div>
        );
    }

    const { meeting } = detail;
    const languageBlocker = readiness === null ? "Checking the models…" : readiness.language;
    const translationBlocker = readiness === null ? "Checking the models…" : readiness.translation;
    const jump = (ms: number) => {
        setTab("transcript");
        setFocusMs(ms);
    };

    async function retry() {
        setBusy("retry");
        try {
            await api.retryMeeting(meeting.id);
            await load();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setBusy(null);
        }
    }

    async function cancelProcessing() {
        setBusy("cancel");
        try {
            await api.cancelMeetingJob(meeting.id);
            await load();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setBusy(null);
        }
    }

    async function remove() {
        setDeleting(true);
        try {
            await api.deleteMeeting(meeting.id);
            setConfirmDelete(false);
            toast(`Deleted “${meeting.title}”`);
            onDeleted();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setDeleting(false);
        }
    }

    return (
        <section className="meeting-view" aria-label={meeting.title}>
            <header className="meeting-view__head">
                <TitleField
                    key={meeting.title}
                    meetingId={meeting.id}
                    title={meeting.title}
                    onRenamed={() => void load()}
                />
                <div className="meeting-view__actions">
                    {(meeting.status === "failed" || meeting.status === "cancelled") && (
                        <Button
                            size="sm"
                            variant="secondary"
                            icon={<RotateCcw />}
                            loading={busy === "retry"}
                            onClick={() => {
                                void retry();
                            }}
                        >
                            Process again
                        </Button>
                    )}
                    <Button
                        size="sm"
                        variant="secondary"
                        icon={<Download />}
                        disabled={meeting.status === "recording"}
                        onClick={() => {
                            setExportOpen(true);
                        }}
                    >
                        Export…
                    </Button>
                    <Button
                        size="sm"
                        variant="ghost"
                        icon={<Trash2 />}
                        disabled={meeting.status === "recording"}
                        onClick={() => {
                            setConfirmDelete(true);
                        }}
                    >
                        Delete
                    </Button>
                </div>
            </header>
            <div className="meeting-view__meta">
                <MeetingStatusBadge status={meeting.status} />
                <span title={formatLongDate(new Date(meeting.startedAt))}>
                    {formatLongDate(new Date(meeting.startedAt))}, {formatTime(meeting.startedAt)}
                </span>
                <span>{formatClock(meeting.durationMs)}</span>
                <span className="meeting-view__sources">
                    {meeting.microphone && <Mic size={13} aria-label="Microphone" />}
                    {meeting.systemAudio && <MonitorSpeaker size={13} aria-label="System audio" />}
                </span>
                {meeting.speakerCount > 0 && (
                    <span>
                        {meeting.speakerCount} {meeting.speakerCount === 1 ? "speaker" : "speakers"}
                    </span>
                )}
            </div>

            {meeting.status === "processing" && (
                <div className="meeting-processing" role="status">
                    {(["transcribe", "diarize"] as const).map((job) => {
                        const progress = processing[job];
                        if (progress === undefined) return null;
                        return (
                            <div key={job} className="meeting-processing__job">
                                <span>
                                    {progress.message ?? PROCESSING_LABEL[job]}
                                    {progress.fraction !== null &&
                                        ` · ${String(Math.round(progress.fraction * 100))}%`}
                                </span>
                                {progress.fraction !== null && (
                                    <ProgressBar value={progress.fraction} />
                                )}
                            </div>
                        );
                    })}
                    {processing.transcribe === undefined && processing.diarize === undefined && (
                        <span className="meeting-processing__job">
                            <Spinner size={13} /> Processing on this Mac…
                        </span>
                    )}
                    <Button
                        size="sm"
                        variant="ghost"
                        loading={busy === "cancel"}
                        onClick={() => {
                            void cancelProcessing();
                        }}
                    >
                        Cancel processing
                    </Button>
                </div>
            )}
            {meeting.status === "failed" && meeting.error && (
                <p className="sv-inline-error meeting-view__error" role="alert">
                    {meeting.error}
                </p>
            )}
            {meeting.status === "cancelled" && (
                <p className="sv-inline-note meeting-view__error">
                    Processing was cancelled. The recording is kept; process it again any time.
                </p>
            )}

            <Tabs
                items={TABS}
                value={tab}
                onChange={(id) => {
                    const next = TABS.find((item) => item.id === id);
                    if (next) setTab(next.id);
                }}
            />
            <div className="meeting-view__body" role="tabpanel">
                {tab === "transcript" && (
                    <TranscriptView
                        detail={detail}
                        focusMs={focusMs}
                        onChanged={() => void load()}
                    />
                )}
                {tab === "summary" && (
                    <SummaryView
                        detail={detail}
                        blocker={languageBlocker}
                        onJump={jump}
                        onChanged={() => void load()}
                    />
                )}
                {tab === "ask" && (
                    <AskView
                        detail={detail}
                        blocker={languageBlocker}
                        onJump={jump}
                        onChanged={() => void load()}
                    />
                )}
                {tab === "translate" && (
                    <TranslateView
                        detail={detail}
                        blocker={translationBlocker}
                        onChanged={() => void load()}
                    />
                )}
            </div>

            {exportOpen && (
                <ExportDialog
                    detail={detail}
                    onClose={() => {
                        setExportOpen(false);
                    }}
                />
            )}
            <Modal
                open={confirmDelete}
                onClose={() => {
                    if (!deleting) setConfirmDelete(false);
                }}
                title="Delete meeting?"
                width={420}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            disabled={deleting}
                            onClick={() => {
                                setConfirmDelete(false);
                            }}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            loading={deleting}
                            onClick={() => {
                                void remove();
                            }}
                        >
                            Delete
                        </Button>
                    </>
                }
            >
                <p className="meeting-confirm">
                    “{meeting.title}”, its recording, transcript, summary, questions and
                    translations are removed from this Mac. This cannot be undone.
                </p>
            </Modal>
        </section>
    );
}
