// FilePath: src/features/meetings/StartMeetingDialog.tsx
// Starting a meeting is explicit: a title, the microphone, and system audio only when the user
// turns it on and macOS allows it. The dialog states the consent the user is giving and refuses
// to start while the local models that process the recording are missing.
import { useState, type SyntheticEvent } from "react";
import { Mic, MonitorSpeaker } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { api, errorMessage, type Meeting, type MeetingReadiness } from "../../lib/api";
import { Button, Modal, TextField, Toggle } from "../../ui";
import { PermissionAction, StatusBadge } from "../settings/permissions/PermissionsGroup";
import { usePermissions } from "../settings/permissions/usePermissions";
import "../settings/common.css";

interface Props {
    readiness: MeetingReadiness | null;
    onClose: () => void;
    onStarted: (meeting: Meeting) => void;
}

const DEFAULT_TITLE = new Intl.DateTimeFormat(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
});

export function StartMeetingDialog({ readiness, onClose, onStarted }: Props) {
    const { settings, update } = useSettings();
    const { permissions, busy, request, openSettings } = usePermissions(1500);
    const [title, setTitle] = useState(() => `Meeting ${DEFAULT_TITLE.format(new Date())}`);
    const [systemAudio, setSystemAudio] = useState(settings.meetingSystemAudio);
    const [starting, setStarting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const microphone = permissions?.microphone;
    const systemAudioStatus = permissions?.systemAudio;
    let blocker: string | null = null;
    if (readiness === null) blocker = "Checking the models…";
    else if (readiness.transcription !== null) blocker = readiness.transcription;
    else if (readiness.diarization !== null) blocker = readiness.diarization;
    else if (microphone !== "granted") blocker = "Simple Voice needs microphone access.";
    else if (systemAudio && systemAudioStatus !== "granted") {
        blocker = "Allow Screen & System Audio Recording, or turn system audio off.";
    }

    async function start(event?: SyntheticEvent) {
        event?.preventDefault();
        if (blocker !== null) return;
        setStarting(true);
        setError(null);
        try {
            const meeting = await api.startMeeting(title.trim(), systemAudio);
            if (systemAudio !== settings.meetingSystemAudio) {
                await update({ meetingSystemAudio: systemAudio });
            }
            onStarted(meeting);
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setStarting(false);
        }
    }

    return (
        <Modal
            open
            onClose={onClose}
            title="Record a meeting"
            width={520}
            footer={
                <>
                    {blocker !== null && <span className="meeting-start__blocker">{blocker}</span>}
                    <Button variant="ghost" onClick={onClose} disabled={starting}>
                        Cancel
                    </Button>
                    <Button
                        variant="primary"
                        loading={starting}
                        disabled={blocker !== null}
                        onClick={() => {
                            void start();
                        }}
                    >
                        Start recording
                    </Button>
                </>
            }
        >
            <form
                className="meeting-start"
                onSubmit={(event) => {
                    void start(event);
                }}
            >
                <TextField
                    label="Title"
                    value={title}
                    autoFocus
                    onChange={(event) => {
                        setTitle(event.target.value);
                    }}
                />
                <div className="meeting-start__source">
                    <Mic size={16} aria-hidden="true" />
                    <div className="meeting-start__source-text">
                        <span className="meeting-start__source-title">Microphone</span>
                        <span className="sv-inline-note">Your voice and anyone in the room.</span>
                    </div>
                    {microphone !== undefined && <StatusBadge status={microphone} />}
                    {microphone !== undefined && microphone !== "granted" && (
                        <PermissionAction
                            kind="microphone"
                            status={microphone}
                            busy={busy === "microphone"}
                            onRequest={request}
                            onOpenSettings={openSettings}
                        />
                    )}
                </div>
                <div className="meeting-start__source">
                    <MonitorSpeaker size={16} aria-hidden="true" />
                    <div className="meeting-start__source-text">
                        <span className="meeting-start__source-title">System audio</span>
                        <span className="sv-inline-note">
                            Other people on a call, from apps like Zoom or Meet. Needs Screen &amp;
                            System Audio Recording; only audio is captured.
                        </span>
                    </div>
                    {systemAudio &&
                        systemAudioStatus !== undefined &&
                        systemAudioStatus !== "granted" && (
                            <PermissionAction
                                kind="system_audio"
                                status={systemAudioStatus}
                                busy={busy === "system_audio"}
                                onRequest={request}
                                onOpenSettings={openSettings}
                            />
                        )}
                    <Toggle
                        label="Also record system audio"
                        checked={systemAudio}
                        onChange={setSystemAudio}
                    />
                </div>
                <p className="meeting-start__consent">
                    Only record people who know they are being recorded. The audio, transcript and
                    everything made from it stay on this Mac until you delete them.
                </p>
                {error && (
                    <p className="sv-inline-error" role="alert">
                        {error}
                    </p>
                )}
                <button type="submit" className="sr-only" aria-hidden="true" tabIndex={-1} />
            </form>
        </Modal>
    );
}
