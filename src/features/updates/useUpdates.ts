// FilePath: src/features/updates/useUpdates.ts
// Live update status. The backend checks on its own schedule and announces every change with
// `update-status`; `check` asks it to check now.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, events, type UpdateStatus } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";

export function useUpdates() {
    const [status, setStatus] = useState<UpdateStatus | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        api.getUpdateStatus().then(
            // An event that arrived first is newer than this snapshot.
            (next) => {
                setStatus((current) => current ?? next);
            },
            (e: unknown) => {
                setError(errorMessage(e));
            },
        );
    }, []);

    useTauriEvent(events.updateStatus, setStatus);

    const check = useCallback(async () => {
        try {
            setStatus(await api.checkForUpdates());
            setError(null);
        } catch (e) {
            setError(errorMessage(e));
        }
    }, []);

    return { status, error: error ?? status?.error ?? null, check };
}
