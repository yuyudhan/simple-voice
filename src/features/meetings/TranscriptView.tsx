// FilePath: src/features/meetings/TranscriptView.tsx
// The timestamped transcript with its speakers. Speaker names and any line can be corrected; a
// name applies to every line of that speaker.
import { useEffect, useState } from "react";
import { Check, Pencil, X } from "lucide-react";
import { api, errorMessage, type MeetingDetail, type Segment, type Speaker } from "../../lib/api";
import { Button, EmptyState, IconButton, TextArea, TextField, useToast } from "../../ui";
import { formatClock } from "./useMeetings";

/** Speakers cycle through this many colour slots (meetings.css); unknown speakers get none. */
const SPEAKER_SLOTS = 6;

/** Colour class for the speaker at `index` in the meeting's speaker list. */
export function speakerTone(index: number | undefined): string {
    return index === undefined
        ? "meeting-speaker--none"
        : `meeting-speaker--${String(index % SPEAKER_SLOTS)}`;
}

export function speakerName(speaker: Speaker | undefined): string {
    if (speaker === undefined) return "Unknown speaker";
    return speaker.name ?? speaker.label;
}

interface ChipProps {
    meetingId: number;
    speaker: Speaker;
    index: number;
    onChanged: () => void;
}

function SpeakerChip({ meetingId, speaker, index, onChanged }: ChipProps) {
    const { toast } = useToast();
    const [editing, setEditing] = useState(false);
    const [name, setName] = useState(speaker.name ?? "");
    const [saving, setSaving] = useState(false);

    async function save() {
        setSaving(true);
        try {
            const trimmed = name.trim();
            await api.renameSpeaker(meetingId, speaker.id, trimmed === "" ? null : trimmed);
            setEditing(false);
            onChanged();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setSaving(false);
        }
    }

    if (editing) {
        return (
            <form
                className="meeting-speaker meeting-speaker--editing"
                onSubmit={(event) => {
                    event.preventDefault();
                    void save();
                }}
            >
                <TextField
                    aria-label={`Name for ${speaker.label}`}
                    placeholder={speaker.label}
                    value={name}
                    autoFocus
                    onChange={(event) => {
                        setName(event.target.value);
                    }}
                />
                <IconButton
                    type="submit"
                    label="Save name"
                    icon={<Check size={14} />}
                    disabled={saving}
                />
                <IconButton
                    label="Cancel"
                    icon={<X size={14} />}
                    onClick={() => {
                        setName(speaker.name ?? "");
                        setEditing(false);
                    }}
                />
            </form>
        );
    }

    return (
        <span className={`meeting-speaker ${speakerTone(index)}`}>
            <span className="meeting-speaker__dot" aria-hidden="true" />
            <span>{speakerName(speaker)}</span>
            {speaker.name !== null && (
                <span className="meeting-speaker__label">{speaker.label}</span>
            )}
            <IconButton
                label={`Rename ${speakerName(speaker)}`}
                icon={<Pencil size={13} />}
                onClick={() => {
                    setEditing(true);
                }}
            />
        </span>
    );
}

interface LineProps {
    segment: Segment;
    speaker: Speaker | undefined;
    /** Position of the line's speaker in the speaker list; undefined when unknown. */
    speakerIndex: number | undefined;
    highlighted: boolean;
    onChanged: () => void;
}

function TranscriptLine({ segment, speaker, speakerIndex, highlighted, onChanged }: LineProps) {
    const { toast } = useToast();
    const [editing, setEditing] = useState(false);
    const [text, setText] = useState(segment.text);
    const [saving, setSaving] = useState(false);

    async function save() {
        if (text.trim().length === 0) {
            toast("A line cannot be empty.", "danger");
            return;
        }
        setSaving(true);
        try {
            await api.updateSegment(segment.id, text);
            setEditing(false);
            onChanged();
        } catch (error) {
            toast(errorMessage(error), "danger");
        } finally {
            setSaving(false);
        }
    }

    return (
        <li
            id={`meeting-segment-${String(segment.id)}`}
            className={`meeting-line${highlighted ? " is-highlighted" : ""}`}
        >
            <span className="meeting-line__time">{formatClock(segment.startMs)}</span>
            <span className={`meeting-line__speaker ${speakerTone(speakerIndex)}`}>
                {speakerName(speaker)}
            </span>
            {editing ? (
                <div className="meeting-line__edit">
                    <TextArea
                        aria-label={`Line at ${formatClock(segment.startMs)}`}
                        value={text}
                        rows={3}
                        autoFocus
                        onChange={(event) => {
                            setText(event.target.value);
                        }}
                    />
                    <div className="meeting-line__edit-actions">
                        <Button
                            size="sm"
                            variant="ghost"
                            disabled={saving}
                            onClick={() => {
                                setText(segment.text);
                                setEditing(false);
                            }}
                        >
                            Cancel
                        </Button>
                        <Button
                            size="sm"
                            variant="primary"
                            loading={saving}
                            onClick={() => {
                                void save();
                            }}
                        >
                            Save
                        </Button>
                    </div>
                </div>
            ) : (
                <p className="meeting-line__text selectable">{segment.text}</p>
            )}
            {!editing && (
                <IconButton
                    className="meeting-line__action"
                    label={`Edit line at ${formatClock(segment.startMs)}`}
                    icon={<Pencil size={13} />}
                    onClick={() => {
                        setText(segment.text);
                        setEditing(true);
                    }}
                />
            )}
        </li>
    );
}

interface Props {
    detail: MeetingDetail;
    /** A moment to scroll to and highlight, e.g. from a summary item or a chat citation. */
    focusMs: number | null;
    /** A speaker or line was saved; reload the meeting. */
    onChanged: () => void;
}

export function TranscriptView({ detail, focusMs, onChanged }: Props) {
    const indexes = new Map(detail.speakers.map((speaker, index) => [speaker.id, index]));
    const speakers = new Map(detail.speakers.map((speaker) => [speaker.id, speaker]));
    // The line containing the focused moment, else the last one starting before it.
    let focused: Segment | undefined;
    if (focusMs !== null) {
        for (const segment of detail.segments) {
            if (segment.startMs > focusMs) break;
            focused = segment;
        }
    }
    const focusedId = focused?.id ?? null;

    useEffect(() => {
        if (focusedId === null) return;
        document
            .getElementById(`meeting-segment-${String(focusedId)}`)
            ?.scrollIntoView({ block: "center", behavior: "smooth" });
    }, [focusedId, focusMs]);

    if (detail.segments.length === 0) {
        return (
            <EmptyState
                icon={<Pencil />}
                title="No transcript yet"
                description={
                    detail.meeting.status === "recording" || detail.meeting.status === "processing"
                        ? "The transcript appears here as it is made on this Mac."
                        : "Nothing was recognised in this recording."
                }
            />
        );
    }

    return (
        <div className="meeting-transcript">
            {detail.speakers.length > 0 && (
                <div className="meeting-speakers" role="group" aria-label="Speakers">
                    {detail.speakers.map((speaker, index) => (
                        <SpeakerChip
                            key={`${String(speaker.id)}-${speaker.name ?? ""}`}
                            meetingId={detail.meeting.id}
                            speaker={speaker}
                            index={index}
                            onChanged={onChanged}
                        />
                    ))}
                </div>
            )}
            <ol className="meeting-lines">
                {detail.segments.map((segment) => (
                    <TranscriptLine
                        key={`${String(segment.id)}-${segment.text}`}
                        segment={segment}
                        onChanged={onChanged}
                        speaker={
                            segment.speakerId === null ? undefined : speakers.get(segment.speakerId)
                        }
                        speakerIndex={
                            segment.speakerId === null ? undefined : indexes.get(segment.speakerId)
                        }
                        highlighted={segment.id === focusedId}
                    />
                ))}
            </ol>
        </div>
    );
}
