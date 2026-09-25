// FilePath: src/features/history/useHistory.ts
import { useCallback, useEffect, useRef, useState } from "react";
import { api, errorMessage, events, type HistoryEntry } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";

const PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 200;

export interface HistoryState {
    entries: HistoryEntry[];
    /** True until the first page for the current query has arrived. */
    loading: boolean;
    loadingMore: boolean;
    hasMore: boolean;
    error: string | null;
    loadMore: () => void;
    replace: (entry: HistoryEntry) => void;
    remove: (id: number) => void;
}

/** Paged history for a search query, kept live through `history-changed`. */
export function useHistory(query: string): HistoryState {
    const [entries, setEntries] = useState<HistoryEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [loadingMore, setLoadingMore] = useState(false);
    const [hasMore, setHasMore] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // Every fetch takes a ticket; responses for an outdated query or reload are ignored.
    const ticket = useRef(0);
    const entriesRef = useRef<HistoryEntry[]>([]);
    const queryRef = useRef(query.trim());
    const busyMore = useRef(false);

    useEffect(() => {
        entriesRef.current = entries;
    }, [entries]);

    const reload = useCallback(async (search: string, limit: number) => {
        ticket.current += 1;
        const mine = ticket.current;
        try {
            const page = await api.listHistory(limit, search.length > 0 ? search : undefined);
            if (mine !== ticket.current) return;
            setEntries(page);
            setHasMore(page.length === limit);
            setError(null);
        } catch (err) {
            if (mine !== ticket.current) return;
            setError(errorMessage(err));
        } finally {
            if (mine === ticket.current) setLoading(false);
        }
    }, []);

    useEffect(() => {
        const search = query.trim();
        queryRef.current = search;
        const timer = window.setTimeout(
            () => {
                void reload(search, PAGE_SIZE);
            },
            search.length > 0 ? SEARCH_DEBOUNCE_MS : 0,
        );
        return () => {
            window.clearTimeout(timer);
        };
    }, [query, reload]);

    useTauriEvent(events.historyChanged, () => {
        // Refetch everything already on screen so the scroll position and expanded rows stay.
        const loaded = entriesRef.current.length;
        const limit = Math.max(PAGE_SIZE, Math.ceil(loaded / PAGE_SIZE) * PAGE_SIZE);
        void reload(queryRef.current, limit);
    });

    const loadMore = useCallback(() => {
        const last = entriesRef.current[entriesRef.current.length - 1];
        if (!last || busyMore.current) return;
        busyMore.current = true;
        const mine = ticket.current;
        const search = queryRef.current;
        setLoadingMore(true);
        void api
            .listHistory(PAGE_SIZE, search.length > 0 ? search : undefined, last.id)
            .then((page) => {
                if (mine !== ticket.current) return;
                setEntries((current) => {
                    const seen = new Set(current.map((entry) => entry.id));
                    return [...current, ...page.filter((entry) => !seen.has(entry.id))];
                });
                setHasMore(page.length === PAGE_SIZE);
            })
            .catch((err: unknown) => {
                if (mine === ticket.current) setError(errorMessage(err));
            })
            .finally(() => {
                busyMore.current = false;
                setLoadingMore(false);
            });
    }, []);

    const replace = useCallback((entry: HistoryEntry) => {
        setEntries((current) => current.map((item) => (item.id === entry.id ? entry : item)));
    }, []);

    const remove = useCallback((id: number) => {
        setEntries((current) => current.filter((item) => item.id !== id));
    }, []);

    return { entries, loading, loadingMore, hasMore, error, loadMore, replace, remove };
}
