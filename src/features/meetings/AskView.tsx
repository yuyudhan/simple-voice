// FilePath: src/features/meetings/AskView.tsx
// Questions about the meeting, answered on this Mac from its transcript only. Each answer cites
// the moments it drew on; a citation jumps to that line.
import { useState, type ReactNode, type SyntheticEvent } from "react";
import { MessagesSquare, Send } from "lucide-react";
import { api, type ChatMessage, type Citation, type MeetingDetail } from "../../lib/api";
import { Button, EmptyState, TextField } from "../../ui";
import { JobProgress } from "./JobProgress";
import { formatClock } from "./useMeetings";
import { useMeetingJob } from "./useMeetingJob";

/** A citation marker: "[1]", "[2, 1]", or the older "[#5]" / "[#5, #7]" segment-id form. */
const MARKER = /\[(#?\d+(?:\s*,\s*#?\d+)*)\]/g;

/** Answer text with each citation marker turned into chips that jump to the cited moment.
 * "[n]" is the n-th citation; "[#id]" (answers saved before numbering) is a segment id. A
 * marker that names no citation stays as plain text. */
function AnswerText({ message, onJump }: { message: ChatMessage; onJump: (ms: number) => void }) {
    const parts: ReactNode[] = [];
    let last = 0;
    for (const match of message.text.matchAll(MARKER)) {
        const refs = (match[1] ?? "").split(",").map((ref) => ref.trim());
        const cited: Citation[] = [];
        for (const ref of refs) {
            const found = ref.startsWith("#")
                ? message.citations.find((c) => c.segmentId === Number(ref.slice(1)))
                : message.citations[Number(ref) - 1];
            if (found) cited.push(found);
        }
        if (cited.length !== refs.length) continue;
        parts.push(message.text.slice(last, match.index));
        parts.push(
            <span key={match.index} className="meeting-message__refs">
                {cited.map((citation, index) => (
                    <button
                        key={`${String(citation.segmentId)}-${String(index)}`}
                        type="button"
                        className="meeting-time-link"
                        title="Show this moment in the transcript"
                        onClick={() => {
                            onJump(citation.startMs);
                        }}
                    >
                        {formatClock(citation.startMs)}
                    </button>
                ))}
            </span>,
        );
        last = match.index + match[0].length;
    }
    parts.push(message.text.slice(last));
    return <p className="meeting-message__text selectable">{parts}</p>;
}

interface Props {
    detail: MeetingDetail;
    /** Why the language model can't run; null when it can. */
    blocker: string | null;
    onJump: (ms: number) => void;
    /** The stored meeting changed; reload it. */
    onChanged: () => void;
}

export function AskView({ detail, blocker, onJump, onChanged }: Props) {
    const { meeting, chat } = detail;
    const job = useMeetingJob(meeting.id, "chat");
    const [question, setQuestion] = useState("");
    const [asked, setAsked] = useState<string | null>(null);

    let why: string | null;
    if (meeting.status !== "ready") why = "Questions can be asked once the transcript is ready.";
    else if (detail.segments.length === 0) why = "There is no transcript to ask about.";
    else why = blocker;

    async function ask(event: SyntheticEvent) {
        event.preventDefault();
        const text = question.trim();
        if (text.length === 0 || why !== null || job.running) return;
        setAsked(text);
        setQuestion("");
        const answer = await job.run(() => api.askMeeting(meeting.id, text));
        setAsked(null);
        // Keep the question to edit and resend when it was not answered.
        if (answer === null) setQuestion(text);
        else onChanged();
    }

    return (
        <div className="meeting-chat">
            {chat.length === 0 && asked === null ? (
                <EmptyState
                    icon={<MessagesSquare />}
                    title="Ask about this meeting"
                    description={
                        why ??
                        "Answers come only from this transcript, with the moments they are based on."
                    }
                />
            ) : (
                <ol className="meeting-chat__messages">
                    {chat.map((message) => (
                        <li
                            key={message.id}
                            className={`meeting-message meeting-message--${message.role}`}
                        >
                            {message.role === "assistant" ? (
                                <AnswerText message={message} onJump={onJump} />
                            ) : (
                                <p className="meeting-message__text selectable">{message.text}</p>
                            )}
                            {message.citations.length > 0 && (
                                <span className="meeting-message__citations">
                                    {message.citations.map((citation) => (
                                        <button
                                            key={citation.segmentId}
                                            type="button"
                                            className="meeting-time-link"
                                            onClick={() => {
                                                onJump(citation.startMs);
                                            }}
                                        >
                                            {formatClock(citation.startMs)}
                                        </button>
                                    ))}
                                </span>
                            )}
                        </li>
                    ))}
                    {asked !== null && (
                        <li className="meeting-message meeting-message--user is-pending">
                            <p className="meeting-message__text">{asked}</p>
                        </li>
                    )}
                </ol>
            )}
            <JobProgress job={job} label="Reading the transcript on this Mac…" />
            {job.error && (
                <p className="sv-inline-error" role="alert">
                    {job.error}
                </p>
            )}
            {why !== null && (chat.length > 0 || asked !== null) && (
                <p className="sv-inline-warning">{why}</p>
            )}
            <form
                className="meeting-chat__form"
                onSubmit={(event) => {
                    void ask(event);
                }}
            >
                <TextField
                    aria-label="Question about this meeting"
                    placeholder="What did we decide about the launch date?"
                    value={question}
                    disabled={why !== null}
                    onChange={(event) => {
                        setQuestion(event.target.value);
                    }}
                />
                <Button
                    type="submit"
                    variant="primary"
                    icon={<Send />}
                    disabled={why !== null || job.running || question.trim().length === 0}
                >
                    Ask
                </Button>
            </form>
        </div>
    );
}
