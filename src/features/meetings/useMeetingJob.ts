// FilePath: src/features/meetings/useMeetingJob.ts
// One long local job on a meeting (summary, answer, translation): whether it runs, its progress
// from `meeting-progress`, its error, and cancelling it. A cancelled job rejects with
// "Cancelled", which is not shown as an error.
import { useCallback, useState } from "react";
import { api, errorMessage, events, type MeetingJob, type MeetingProgress } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";

const CANCELLED = "Cancelled";

export interface MeetingJobState {
    running: boolean;
    progress: MeetingProgress | null;
    error: string | null;
    cancelling: boolean;
    /** Runs `task`; resolves with its result, or null when it failed or was cancelled. */
    run: <T>(task: () => Promise<T>) => Promise<T | null>;
    cancel: () => Promise<void>;
}

export function useMeetingJob(meetingId: number, job: MeetingJob): MeetingJobState {
    const [running, setRunning] = useState(false);
    const [progress, setProgress] = useState<MeetingProgress | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [cancelling, setCancelling] = useState(false);

    useTauriEvent(events.meetingProgress, (next) => {
        if (next.id === meetingId && next.job === job) setProgress(next);
    });

    const run = useCallback(async <T>(task: () => Promise<T>): Promise<T | null> => {
        setRunning(true);
        setError(null);
        setProgress(null);
        try {
            return await task();
        } catch (e) {
            const message = errorMessage(e);
            if (message !== CANCELLED) setError(message);
            return null;
        } finally {
            setRunning(false);
            setCancelling(false);
            setProgress(null);
        }
    }, []);

    const cancel = useCallback(async () => {
        setCancelling(true);
        try {
            await api.cancelMeetingJob(meetingId);
        } catch (e) {
            setCancelling(false);
            setError(errorMessage(e));
        }
    }, [meetingId]);

    return { running, progress, error, cancelling, run, cancel };
}
