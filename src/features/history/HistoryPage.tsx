// FilePath: src/features/history/HistoryPage.tsx
import { useEffect, useMemo, useRef, useState } from "react";
import { Mic, Search, SearchX } from "lucide-react";
import { api, errorMessage, type HistoryEntry } from "../../lib/api";
import { dayKey, dayLabel } from "../../lib/format";
import { useSettings } from "../../app/SettingsContext";
import {
    Button,
    EmptyState,
    Kbd,
    Modal,
    ShortcutKeys,
    Spinner,
    TextField,
    useToast,
} from "../../ui";
import { HistoryRow } from "./HistoryRow";
import { StatsStrip } from "./StatsStrip";
import { useHistory } from "./useHistory";
import "./history.css";

interface DayGroup {
    key: string;
    label: string;
    entries: HistoryEntry[];
}

const EMPTY_DESCRIPTION =
    "Put the cursor in any text field, speak, and Simple Voice pastes the words for you.";

export function HistoryPage() {
    const { settings } = useSettings();
    const { toast } = useToast();
    const [query, setQuery] = useState("");
    const history = useHistory(query);
    const { entries, hasMore, loadingMore, loadMore } = history;
    const [pendingDelete, setPendingDelete] = useState<HistoryEntry | null>(null);
    const [deleting, setDeleting] = useState(false);
    const [retrying, setRetrying] = useState<ReadonlySet<number>>(new Set());
    const sentinelRef = useRef<HTMLDivElement>(null);

    const groups = useMemo(() => {
        const result: DayGroup[] = [];
        for (const entry of entries) {
            const date = new Date(entry.createdAt);
            const key = dayKey(date);
            const last = result[result.length - 1];
            if (last?.key === key) last.entries.push(entry);
            else result.push({ key, label: dayLabel(date), entries: [entry] });
        }
        return result;
    }, [entries]);

    useEffect(() => {
        const sentinel = sentinelRef.current;
        if (!sentinel || !hasMore) return undefined;
        const observer = new IntersectionObserver(
            (records) => {
                if (records.some((record) => record.isIntersecting)) loadMore();
            },
            { rootMargin: "400px 0px" },
        );
        observer.observe(sentinel);
        return () => {
            observer.disconnect();
        };
    }, [hasMore, loadMore, entries.length]);

    async function copy(entry: HistoryEntry) {
        try {
            await api.copyText(entry.text.trim().length > 0 ? entry.text : entry.rawText);
            toast("Copied to clipboard", "success");
        } catch (error) {
            toast(errorMessage(error), "danger");
        }
    }

    async function retry(entry: HistoryEntry) {
        setRetrying((current) => new Set(current).add(entry.id));
        try {
            const updated = await api.retryHistory(entry.id);
            history.replace(updated);
            toast(
                updated.status === "failed" ? "Retry failed again" : "Transcribed",
                updated.status === "failed" ? "danger" : "success",
            );
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setRetrying((current) => {
                const next = new Set(current);
                next.delete(entry.id);
                return next;
            });
        }
    }

    async function confirmDelete() {
        if (!pendingDelete) return;
        setDeleting(true);
        try {
            await api.deleteHistory(pendingDelete.id);
            history.remove(pendingDelete.id);
            setPendingDelete(null);
            toast("Dictation deleted");
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setDeleting(false);
        }
    }

    const searching = query.trim().length > 0;

    return (
        <>
            <header className="sv-page-header">
                <div>
                    <h1 className="page-title">Your dictations</h1>
                    <p className="page-subtitle">Everything you have said, newest first.</p>
                </div>
            </header>

            <StatsStrip />

            <div className="history-search">
                <Search className="history-search__icon" aria-hidden="true" />
                <TextField
                    type="search"
                    placeholder="Search dictations"
                    aria-label="Search dictations"
                    value={query}
                    onChange={(event) => {
                        setQuery(event.target.value);
                    }}
                />
            </div>

            {history.error && entries.length === 0 && (
                <p className="history-error" role="alert">
                    {history.error}
                </p>
            )}

            {history.loading ? (
                <div className="history-loading">
                    <Spinner size={20} />
                </div>
            ) : entries.length === 0 ? (
                searching ? (
                    <EmptyState
                        icon={<SearchX />}
                        title="No matches"
                        description={`Nothing you dictated contains “${query.trim()}”.`}
                    />
                ) : (
                    <EmptyState
                        icon={<Mic />}
                        title="Nothing dictated yet"
                        description={EMPTY_DESCRIPTION}
                        action={
                            <div className="history-howto">
                                <div>
                                    Hold <ShortcutKeys accelerator={settings.holdShortcut} /> while
                                    you speak and let go to paste.
                                </div>
                                <div>
                                    Or press <ShortcutKeys accelerator={settings.toggleShortcut} />{" "}
                                    to start, and again to stop. <Kbd>Esc</Kbd> cancels.
                                </div>
                            </div>
                        }
                    />
                )
            ) : (
                <div className="history-groups">
                    {groups.map((group) => (
                        <section key={group.key} className="history-group" aria-label={group.label}>
                            <h2 className="history-group__label caps-label">{group.label}</h2>
                            <div className="history-group__rows">
                                {group.entries.map((entry) => (
                                    <HistoryRow
                                        key={entry.id}
                                        entry={entry}
                                        retrying={retrying.has(entry.id)}
                                        onCopy={(item) => {
                                            void copy(item);
                                        }}
                                        onRetry={(item) => {
                                            void retry(item);
                                        }}
                                        onDelete={setPendingDelete}
                                    />
                                ))}
                            </div>
                        </section>
                    ))}
                    <div ref={sentinelRef} className="history-sentinel" aria-hidden="true">
                        {loadingMore && <Spinner size={16} />}
                    </div>
                </div>
            )}

            <Modal
                open={pendingDelete !== null}
                onClose={() => {
                    if (!deleting) setPendingDelete(null);
                }}
                title="Delete this dictation?"
                width={420}
                footer={
                    <>
                        <Button
                            variant="ghost"
                            onClick={() => {
                                setPendingDelete(null);
                            }}
                            disabled={deleting}
                        >
                            Cancel
                        </Button>
                        <Button
                            variant="danger"
                            loading={deleting}
                            onClick={() => {
                                void confirmDelete();
                            }}
                        >
                            Delete
                        </Button>
                    </>
                }
            >
                <p className="history-confirm">
                    The text and any saved audio are removed from this Mac. This cannot be undone.
                </p>
                {pendingDelete && (
                    <blockquote className="history-confirm__quote">
                        {pendingDelete.text.trim().length > 0
                            ? pendingDelete.text
                            : pendingDelete.rawText}
                    </blockquote>
                )}
            </Modal>
        </>
    );
}
