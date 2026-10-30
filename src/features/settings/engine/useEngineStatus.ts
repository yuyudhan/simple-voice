// FilePath: src/features/settings/engine/useEngineStatus.ts
// The speech engine's live status and a way to start it now.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, events, type EngineStatus } from "../../../lib/api";
import { useTauriEvent } from "../../../lib/useTauriEvent";

export interface EngineStatusState {
    status: EngineStatus | null;
    restarting: boolean;
    error: string | null;
    restart: () => Promise<void>;
}

export function useEngineStatus(): EngineStatusState {
    const [status, setStatus] = useState<EngineStatus | null>(null);
    const [restarting, setRestarting] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;
        api.engineStatus()
            .then((loaded) => {
                // An event that arrived meanwhile is newer than this answer.
                if (!cancelled) setStatus((current) => current ?? loaded);
            })
            .catch((e: unknown) => {
                if (!cancelled) setError(errorMessage(e));
            });
        return () => {
            cancelled = true;
        };
    }, []);

    useTauriEvent(events.engineStatus, setStatus);

    const restart = useCallback(async () => {
        setRestarting(true);
        setError(null);
        try {
            setStatus(await api.restartEngine());
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setRestarting(false);
        }
    }, []);

    return { status, restarting, error, restart };
}
