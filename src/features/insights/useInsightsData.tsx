// FilePath: src/features/insights/useInsightsData.tsx
import { useCallback, useEffect, useState } from "react";
import { errorMessage, events } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";
import { Spinner, useToast } from "../../ui";

export interface InsightsData<T> {
    data: T | null;
    failed: string | null;
}

/** Loads an Insights aggregate and reloads it whenever history changes. */
export function useInsightsData<T>(fetch: () => Promise<T>): InsightsData<T> {
    const { toast } = useToast();
    const [data, setData] = useState<T | null>(null);
    const [failed, setFailed] = useState<string | null>(null);

    const load = useCallback(
        () =>
            fetch().then(
                (next) => {
                    setData(next);
                    setFailed(null);
                },
                (error: unknown) => {
                    setFailed(errorMessage(error));
                    toast(errorMessage(error), "danger");
                },
            ),
        [fetch, toast],
    );

    useEffect(() => {
        void load();
    }, [load]);

    useTauriEvent(events.historyChanged, () => {
        void load();
    });

    return { data, failed };
}

/** What a tab shows before its first load finishes: the error, or a spinner. */
export function InsightsPending({ failed }: { failed: string | null }) {
    return failed ? (
        <p className="insights-error" role="alert">
            {failed}
        </p>
    ) : (
        <div className="insights-loading">
            <Spinner size={20} />
        </div>
    );
}
