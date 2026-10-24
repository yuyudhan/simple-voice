// FilePath: src/features/meetings/MeetingsPage.tsx
// Personal meetings, recorded and processed entirely on this Mac: a list of meetings beside the
// selected one, the recording in progress with Stop and Discard, and the models meetings need.
import { useState } from "react";
import { Circle, Mic, Settings2, Square, Users, X } from "lucide-react";
import { api, errorMessage, type Meeting } from "../../lib/api";
import { formatTime, dayLabel } from "../../lib/format";
import { Button, EmptyState, Modal, PageHeader, Spinner, useToast } from "../../ui";
import { useModels } from "../settings/models/useModels";
import { MeetingSetup } from "./MeetingSetup";
import { MeetingStatusBadge } from "./MeetingStatusBadge";
import { MeetingView } from "./MeetingView";
import { StartMeetingDialog } from "./StartMeetingDialog";
import { formatClock, useMeetings, type MeetingsState } from "./useMeetings";
import "../settings/common.css";
import "./meetings.css";
import "./meetingContent.css";

const DESCRIPTION =
    "Record a conversation, then read it by speaker, summarise it, ask about it and translate " +
    "it. Everything happens on this Mac.";

function RecordingBar({ recording }: { recording: NonNullable<MeetingsState["recording"]> }) {
    const { toast } = useToast();
    const [busy, setBusy] = useState<"stop" | "discard" | null>(null);
    const [confirmDiscard, setConfirmDiscard] = useState(false);

    async function stop() {
        setBusy("stop");
        try {
            await api.stopMeeting();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setBusy(null);
        }
    }

    async function discard() {
        setBusy("discard");
        try {
            await api.cancelMeeting();
            setConfirmDiscard(false);
            toast("Recording discarded");
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setBusy(null);
        }
    }

    return (
        <div className="meeting-recording" role="status" aria-live="polite">
            <span className="meeting-recording__dot" aria-hidden="true" />
            <span className="meeting-recording__title">{recording.meeting.title}</span>
            <span className="meeting-recording__clock readout">
                {formatClock(recording.elapsedMs)}
            </span>
            <span className="meeting-recording__sources">
                <Mic size={13} aria-label="Microphone" />
                {recording.meeting.systemAudio && " + system audio"}
            </span>
            <span className="meeting-recording__spacer" />
            <Button
                variant="ghost"
                size="sm"
                icon={<X />}
                disabled={busy !== null}
                onClick={() => {
                    setConfirmDiscard(true);
                }}
            >
                Discard
            </Button>
            <Button
                variant="primary"
                size="sm"
                icon={<Square />}
                loading={busy === "stop"}
                disabled={busy === "discard"}
                onClick={() => {
                    void stop();
                }}
            >
                Stop and process
            </Button>
            <Modal
                open={confirmDiscard}
                onClose={() => {
                    if (busy === null) setConfirmDiscard(false);
                }}
                title="Discard this recording?"
                width={420}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            disabled={busy !== null}
                            onClick={() => {
                                setConfirmDiscard(false);
                            }}
                        >
                            Keep recording
                        </Button>
                        <Button
                            variant="danger"
                            loading={busy === "discard"}
                            onClick={() => {
                                void discard();
                            }}
                        >
                            Discard
                        </Button>
                    </>
                }
            >
                <p className="meeting-confirm">
                    Recording stops and the audio so far is deleted. Nothing is transcribed.
                </p>
            </Modal>
        </div>
    );
}

function MeetingRow({
    meeting,
    selected,
    now,
    onSelect,
}: {
    meeting: Meeting;
    selected: boolean;
    now: Date;
    onSelect: (id: number) => void;
}) {
    return (
        <li>
            <button
                type="button"
                className={`meeting-row${selected ? " is-selected" : ""}`}
                aria-current={selected ? "true" : undefined}
                onClick={() => {
                    onSelect(meeting.id);
                }}
            >
                <span className="meeting-row__title">{meeting.title}</span>
                <span className="meeting-row__meta">
                    {dayLabel(new Date(meeting.startedAt), now, { weekday: false })},{" "}
                    {formatTime(meeting.startedAt)} · {formatClock(meeting.durationMs)}
                </span>
                {meeting.status !== "ready" && <MeetingStatusBadge status={meeting.status} />}
            </button>
        </li>
    );
}

export function MeetingsPage() {
    const state = useMeetings();
    const models = useModels();
    const [selected, setSelected] = useState<number | null>(null);
    const [starting, setStarting] = useState(false);
    const [setupOpen, setSetupOpen] = useState(false);
    const { meetings, recording, readiness } = state;

    const current =
        meetings?.find((m) => m.id === selected) ?? (selected === null ? meetings?.[0] : undefined);
    const now = new Date();
    const notReady =
        readiness !== null && (readiness.transcription !== null || readiness.diarization !== null);

    return (
        <>
            <PageHeader
                title="Meetings"
                description={DESCRIPTION}
                actions={
                    <>
                        <Button
                            variant="secondary"
                            icon={<Settings2 />}
                            aria-expanded={setupOpen}
                            onClick={() => {
                                setSetupOpen((open) => !open);
                            }}
                        >
                            Models
                        </Button>
                        <Button
                            variant="primary"
                            icon={<Circle />}
                            disabled={recording !== null}
                            onClick={() => {
                                setStarting(true);
                            }}
                        >
                            Record meeting
                        </Button>
                    </>
                }
            />

            {recording !== null && <RecordingBar recording={recording} />}

            {(setupOpen || notReady) && (
                <MeetingSetup
                    models={models}
                    readiness={readiness}
                    readinessError={state.readinessError}
                />
            )}

            {meetings === null ? (
                <div className="meeting-loading">
                    {state.loadError ? (
                        <p className="sv-inline-error" role="alert">
                            {state.loadError}
                        </p>
                    ) : (
                        <Spinner size={20} />
                    )}
                </div>
            ) : meetings.length === 0 ? (
                <EmptyState
                    icon={<Users />}
                    title="No meetings yet"
                    description="Record a call or a conversation. You choose whether system audio is included, and the recording never leaves this Mac."
                    action={
                        <Button
                            variant="primary"
                            icon={<Circle />}
                            disabled={recording !== null}
                            onClick={() => {
                                setStarting(true);
                            }}
                        >
                            Record your first meeting
                        </Button>
                    }
                />
            ) : (
                <div className="meetings-layout">
                    <nav className="meetings-list" aria-label="Meetings">
                        <ul>
                            {meetings.map((meeting) => (
                                <MeetingRow
                                    key={meeting.id}
                                    meeting={meeting}
                                    selected={meeting.id === current?.id}
                                    now={now}
                                    onSelect={setSelected}
                                />
                            ))}
                        </ul>
                    </nav>
                    {current ? (
                        <MeetingView
                            key={current.id}
                            meetingId={current.id}
                            readiness={readiness}
                            onDeleted={() => {
                                void state.reload().then(() => {
                                    setSelected(null);
                                });
                            }}
                        />
                    ) : (
                        <EmptyState icon={<Users />} title="Pick a meeting" />
                    )}
                </div>
            )}

            {starting && (
                <StartMeetingDialog
                    readiness={readiness}
                    onClose={() => {
                        setStarting(false);
                    }}
                    onStarted={(meeting) => {
                        setStarting(false);
                        setSelected(meeting.id);
                        void state.reload();
                    }}
                />
            )}
        </>
    );
}
