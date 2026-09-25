// FilePath: src/features/history/HistoryRow.tsx
import { useEffect, useRef, useState } from "react";
import { Copy, RotateCcw, Trash2 } from "lucide-react";
import type { HistoryEntry } from "../../lib/api";
import { CATEGORY_META } from "../../lib/categories";
import { formatTime } from "../../lib/format";
import { Badge, IconButton, Spinner } from "../../ui";

export interface HistoryRowProps {
    entry: HistoryEntry;
    retrying: boolean;
    onCopy: (entry: HistoryEntry) => void;
    onRetry: (entry: HistoryEntry) => void;
    onDelete: (entry: HistoryEntry) => void;
}

function StatusBadge({ entry }: { entry: HistoryEntry }) {
    switch (entry.status) {
        case "pasted":
            return null;
        case "unformatted":
            return <Badge tone="warning">Unformatted</Badge>;
        case "failed":
            return <Badge tone="danger">Failed</Badge>;
        case "dropped":
            return <Badge tone="neutral">Not pasted</Badge>;
    }
}

export function HistoryRow({ entry, retrying, onCopy, onRetry, onDelete }: HistoryRowProps) {
    const textRef = useRef<HTMLParagraphElement>(null);
    const [expanded, setExpanded] = useState(false);
    const [overflowing, setOverflowing] = useState(false);
    const category = CATEGORY_META[entry.appCategory];
    const text = entry.text.trim().length > 0 ? entry.text : entry.rawText;

    // Only offer "Show more" when the clamped text is actually cut off at the current width.
    useEffect(() => {
        const node = textRef.current;
        if (!node || expanded) return undefined;
        const observer = new ResizeObserver(() => {
            setOverflowing(node.scrollHeight > node.clientHeight + 1);
        });
        observer.observe(node);
        return () => {
            observer.disconnect();
        };
    }, [expanded, text]);

    return (
        <article className={`history-row history-row--${entry.status}`}>
            <time className="history-row__time" dateTime={new Date(entry.createdAt).toISOString()}>
                {formatTime(entry.createdAt)}
            </time>

            <div className="history-row__body">
                <div className="history-row__meta">
                    <span className="history-row__app" title={category.label}>
                        {category.icon}
                        {entry.appName ?? category.label}
                    </span>
                    <StatusBadge entry={entry} />
                </div>

                {text.trim().length > 0 ? (
                    <p
                        ref={textRef}
                        className={`history-row__text selectable${expanded ? " is-expanded" : ""}`}
                    >
                        {text}
                    </p>
                ) : (
                    <p className="history-row__text history-row__text--empty">
                        No speech was recognised.
                    </p>
                )}

                {(overflowing || expanded) && (
                    <button
                        type="button"
                        className="history-row__more"
                        aria-expanded={expanded}
                        onClick={() => {
                            setExpanded((value) => !value);
                        }}
                    >
                        {expanded ? "Show less" : "Show more"}
                    </button>
                )}

                {entry.status === "failed" && entry.error && (
                    <p className="history-row__error selectable">{entry.error}</p>
                )}
                {entry.status === "dropped" && (
                    <p className="history-row__note">
                        A newer dictation was pasted first, so this one was kept here instead.
                    </p>
                )}
                {entry.status === "unformatted" && (
                    <p className="history-row__note">
                        AI formatting was skipped{entry.error ? `: ${entry.error}` : "."}
                    </p>
                )}
            </div>

            <div className="history-row__actions">
                {text.trim().length > 0 && (
                    <IconButton
                        label="Copy text"
                        icon={<Copy />}
                        onClick={() => {
                            onCopy(entry);
                        }}
                    />
                )}
                {entry.status === "failed" && entry.canRetry && (
                    <IconButton
                        label="Retry transcription"
                        icon={retrying ? <Spinner size={14} /> : <RotateCcw />}
                        disabled={retrying}
                        onClick={() => {
                            onRetry(entry);
                        }}
                    />
                )}
                <IconButton
                    label="Delete dictation"
                    icon={<Trash2 />}
                    onClick={() => {
                        onDelete(entry);
                    }}
                />
            </div>
        </article>
    );
}
