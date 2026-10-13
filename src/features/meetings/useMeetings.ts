// FilePath: src/features/meetings/useMeetings.ts
// The meeting list, the recording in progress and the readiness of the local models meetings
// need. The list follows `meetings-changed`; the recording's clock follows `meeting-state`.
import { useCallback, useEffect, useState } from "react";
import {
    api,
    errorMessage,
    events,
    type Meeting,
    type MeetingReadiness,
    type MeetingState,
} from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";

/** "1:05" or "1:02:05": a transcript timestamp or a duration. */
export function formatClock(ms: number): string {
    const total = Math.max(0, Math.floor(ms / 1000));
    const hours = Math.floor(total / 3600);
    const minutes = Math.floor((total % 3600) / 60);
    const seconds = String(total % 60).padStart(2, "0");
    if (hours > 0) return `${String(hours)}:${String(minutes).padStart(2, "0")}:${seconds}`;
    return `${String(minutes)}:${seconds}`;
}

export interface MeetingsState {
    meetings: Meeting[] | null;
    loadError: string | null;
    /** The meeting being recorded, with its elapsed time; null when nothing records. */
    recording: { meeting: Meeting; elapsedMs: number } | null;
    readiness: MeetingReadiness | null;
    readinessError: string | null;
    reload: () => Promise<void>;
    reloadReadiness: () => Promise<void>;
}

export function useMeetings(): MeetingsState {
    const [meetings, setMeetings] = useState<Meeting[] | null>(null);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [live, setLive] = useState<MeetingState | null>(null);
    const [readiness, setReadiness] = useState<MeetingReadiness | null>(null);
    const [readinessError, setReadinessError] = useState<string | null>(null);

    const reload = useCallback(
        () =>
            api.listMeetings().then(
                (list) => {
                    setMeetings(list);
                    setLoadError(null);
                },
                (error: unknown) => {
                    setLoadError(errorMessage(error));
                },
            ),
        [],
    );

    const reloadReadiness = useCallback(
        () =>
            api.meetingReadiness().then(
                (next) => {
                    setReadiness(next);
                    setReadinessError(null);
                },
                (error: unknown) => {
                    setReadinessError(errorMessage(error));
                },
            ),
        [],
    );

    useEffect(() => {
        void reload();
        void reloadReadiness();
    }, [reload, reloadReadiness]);

    useTauriEvent(events.meetingsChanged, () => {
        void reload();
    });
    // A model downloaded or deleted, or a new model choice, can change whether a stage is ready.
    useTauriEvent(events.modelsChanged, () => {
        void reloadReadiness();
    });
    useTauriEvent(events.settingsChanged, () => {
        void reloadReadiness();
    });

    useTauriEvent(events.meetingState, (state) => {
        setLive(state.status === "recording" ? state : null);
        const known = meetings?.find((m) => m.id === state.id);
        if (known === undefined || state.status !== "recording") {
            // A meeting started elsewhere, or one that changed stage: the list carries its
            // details (end time, segment and speaker counts).
            void reload();
        }
        if (known !== undefined && known.status !== state.status) {
            setMeetings((current) =>
                (current ?? []).map((m) =>
                    m.id === state.id ? { ...m, status: state.status } : m,
                ),
            );
        }
    });

    const recordingMeeting = meetings?.find((m) => m.status === "recording");
    const recording: MeetingsState["recording"] =
        recordingMeeting === undefined
            ? null
            : {
                  meeting: recordingMeeting,
                  elapsedMs:
                      live?.id === recordingMeeting.id
                          ? live.elapsedMs
                          : recordingMeeting.durationMs,
              };

    return {
        meetings,
        loadError,
        recording,
        readiness,
        readinessError,
        reload,
        reloadReadiness,
    };
}
