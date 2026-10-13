// FilePath: src/app/LiveTranscript.tsx
// The words recognised so far while a dictation records, from a streaming voice model. Settled
// words never change; the tentative tail may. It disappears when recording ends: the pasted text
// is the final result, and nothing here is kept.
import { useState } from "react";
import { events, type DictationPartial } from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import "./LiveTranscript.css";

export function LiveTranscript() {
    const [partial, setPartial] = useState<DictationPartial | null>(null);

    useTauriEvent(events.dictationPartial, setPartial);
    useTauriEvent(events.dictationState, (state) => {
        if (state.phase !== "recording") setPartial(null);
        else setPartial((current) => (current?.sessionId === state.sessionId ? current : null));
    });

    if (partial === null || (partial.text === "" && partial.tentative === "")) return null;
    return (
        <div className="sv-live" role="status" aria-live="polite">
            <span className="sv-live__label caps-label">Live</span>
            <div className="sv-live__text">
                <p>
                    {partial.text}
                    {partial.text !== "" && partial.tentative !== "" && " "}
                    <span className="sv-live__tentative">{partial.tentative}</span>
                </p>
            </div>
        </div>
    );
}
