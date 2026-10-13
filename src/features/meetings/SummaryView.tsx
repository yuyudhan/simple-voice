// FilePath: src/features/meetings/SummaryView.tsx
// The summary and action items, written on this Mac from the transcript. Each action item that
// came from a moment links back to it in the transcript.
import { ListTodo, NotebookText, RefreshCw } from "lucide-react";
import { api, type MeetingDetail } from "../../lib/api";
import { Button, EmptyState } from "../../ui";
import { JobProgress } from "./JobProgress";
import { formatClock } from "./useMeetings";
import { useMeetingJob } from "./useMeetingJob";

interface Props {
    detail: MeetingDetail;
    /** Why the language model can't run; null when it can. */
    blocker: string | null;
    onJump: (ms: number) => void;
    /** The stored meeting changed; reload it. */
    onChanged: () => void;
}

export function SummaryView({ detail, blocker, onJump, onChanged }: Props) {
    const { meeting, summary } = detail;
    const job = useMeetingJob(meeting.id, "summarize");
    let why: string | null;
    if (meeting.status !== "ready") why = "A summary can be written once the transcript is ready.";
    else if (detail.segments.length === 0) why = "There is no transcript to summarise.";
    else why = blocker;

    const action = (
        <Button
            variant={summary === null ? "primary" : "secondary"}
            size={summary === null ? "md" : "sm"}
            icon={summary === null ? <NotebookText /> : <RefreshCw />}
            disabled={why !== null || job.running}
            onClick={() => {
                void job
                    .run(() => api.summarizeMeeting(meeting.id))
                    .then((written) => {
                        if (written !== null) onChanged();
                    });
            }}
        >
            {summary === null ? "Write summary" : "Rewrite"}
        </Button>
    );

    return (
        <div className="meeting-summary">
            <JobProgress job={job} label="Writing the summary on this Mac…" />
            {job.error && (
                <p className="sv-inline-error" role="alert">
                    {job.error}
                </p>
            )}
            {summary === null ? (
                !job.running && (
                    <EmptyState
                        icon={<NotebookText />}
                        title="No summary yet"
                        description={why ?? "Summarise the meeting and list its action items."}
                        action={action}
                    />
                )
            ) : (
                <>
                    <div className="meeting-summary__head">
                        <span className="meeting-summary__meta">
                            Written by {summary.model} on this Mac
                        </span>
                        {action}
                    </div>
                    {why !== null && <p className="sv-inline-warning">{why}</p>}
                    <p className="meeting-summary__text selectable">{summary.text}</p>
                    <h3 className="meeting-summary__title caps-label">
                        <ListTodo size={14} aria-hidden="true" /> Action items
                    </h3>
                    {summary.actionItems.length === 0 ? (
                        <p className="sv-inline-note">No action items were found.</p>
                    ) : (
                        <ul className="meeting-actions">
                            {summary.actionItems.map((item, index) => (
                                <li
                                    key={`${String(index)}-${item.text}`}
                                    className="meeting-action"
                                >
                                    <span className="meeting-action__text selectable">
                                        {item.text}
                                    </span>
                                    {item.owner !== null && (
                                        <span className="meeting-action__owner">{item.owner}</span>
                                    )}
                                    {item.startMs !== null && (
                                        <button
                                            type="button"
                                            className="meeting-time-link"
                                            onClick={() => {
                                                if (item.startMs !== null) onJump(item.startMs);
                                            }}
                                        >
                                            {formatClock(item.startMs)}
                                        </button>
                                    )}
                                </li>
                            ))}
                        </ul>
                    )}
                </>
            )}
        </div>
    );
}
