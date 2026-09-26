// FilePath: src/features/overlay/Overlay.tsx
// The floating dictation pill rendered in the transparent `overlay` window. Level bars are driven
// straight from a requestAnimationFrame loop into the DOM so the ~30 Hz level stream never causes
// React re-renders; React only re-renders on phase changes and the once-a-second timer.
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Mic, PenLine, TriangleAlert } from "lucide-react";
import { events, type DictationState } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";
import "./Overlay.css";

const BAR_COUNT = 11;
const BAR_MIN_PX = 3;
const BAR_MAX_PX = 22;
// Centre bars reach higher than the edges, like a voice envelope.
const BAR_PROFILE = Array.from({ length: BAR_COUNT }, (_, i) => {
    const x = (i - (BAR_COUNT - 1) / 2) / ((BAR_COUNT - 1) / 2);
    return 0.4 + 0.6 * Math.cos((x * Math.PI) / 2);
});
const ATTACK = 0.45;
const DECAY = 0.12;
// Bars above this share of their range are the loudest and light up in signal orange.
const HOT_LEVEL = 0.62;

// How long a finished phase stays on screen before the pill settles back to idle.
const HOLD_MS: Partial<Record<DictationState["phase"], number>> = {
    done: 1100,
    error: 2000,
    cancelled: 350,
};

function formatElapsed(ms: number): string {
    const total = Math.max(0, Math.floor(ms / 1000));
    const seconds = total % 60;
    return `${String(Math.floor(total / 60))}:${seconds < 10 ? "0" : ""}${String(seconds)}`;
}

function LevelBars({ live, shimmer }: { live: boolean; shimmer: boolean }) {
    const barsRef = useRef<(HTMLSpanElement | null)[]>([]);
    const levelRef = useRef(0);

    const onLevel = useCallback((level: number) => {
        levelRef.current = level;
    }, []);
    useTauriEvent(events.dictationLevel, onLevel);

    useEffect(() => {
        const bars = barsRef.current;
        if (!live) {
            levelRef.current = 0;
            for (const bar of bars) {
                bar?.style.removeProperty("height");
                bar?.classList.remove("is-hot");
            }
            return;
        }
        const heights = new Array<number>(BAR_COUNT).fill(0);
        let frame = 0;
        const tick = (time: number) => {
            // Perceived loudness is closer to the square root of the RMS level.
            const target = Math.min(1, Math.sqrt(levelRef.current) * 1.25);
            for (let i = 0; i < BAR_COUNT; i += 1) {
                const wobble = 0.72 + 0.28 * Math.sin(time / (140 + i * 23) + i * 1.7);
                const desired = target * (BAR_PROFILE[i] ?? 1) * wobble;
                const current = heights[i] ?? 0;
                const next = current + (desired - current) * (desired > current ? ATTACK : DECAY);
                heights[i] = next;
                const bar = bars[i];
                const px = BAR_MIN_PX + next * (BAR_MAX_PX - BAR_MIN_PX);
                if (bar) {
                    bar.style.height = `${px.toFixed(1)}px`;
                    bar.classList.toggle("is-hot", next > HOT_LEVEL);
                }
            }
            frame = requestAnimationFrame(tick);
        };
        frame = requestAnimationFrame(tick);
        return () => {
            cancelAnimationFrame(frame);
        };
    }, [live]);

    const modifiers = `${live ? " is-live" : ""}${shimmer ? " is-shimmer" : ""}`;
    return (
        <span className={`sv-pill__bars${modifiers}`} aria-hidden="true">
            {BAR_PROFILE.map((_, i) => (
                <span
                    key={i}
                    className="sv-pill__bar"
                    ref={(el) => {
                        barsRef.current[i] = el;
                    }}
                />
            ))}
        </span>
    );
}

function Timer({ startedAt }: { startedAt: number }) {
    const [now, setNow] = useState(() => Date.now());
    useEffect(() => {
        const timer = window.setInterval(() => {
            setNow(Date.now());
        }, 250);
        return () => {
            window.clearInterval(timer);
        };
    }, []);
    return <span className="sv-pill__timer">{formatElapsed(now - startedAt)}</span>;
}

export function Overlay() {
    const [state, setState] = useState<DictationState>({ phase: "idle", sessionId: 0 });
    const [recordingSince, setRecordingSince] = useState<number>(() => Date.now());

    useEffect(() => {
        document.documentElement.classList.add("overlay-window");
        return () => {
            document.documentElement.classList.remove("overlay-window");
        };
    }, []);

    const onState = useCallback((next: DictationState) => {
        if (next.phase === "recording") setRecordingSince(next.startedAt ?? Date.now());
        setState(next);
    }, []);
    useTauriEvent(events.dictationState, onState);

    useEffect(() => {
        const hold = HOLD_MS[state.phase];
        if (hold === undefined) return;
        const timer = window.setTimeout(() => {
            setState((current) =>
                current.sessionId === state.sessionId && current.phase === state.phase
                    ? { phase: "idle", sessionId: current.sessionId }
                    : current,
            );
        }, hold);
        return () => {
            window.clearTimeout(timer);
        };
    }, [state.phase, state.sessionId]);

    const { phase, edit = false } = state;
    const busy = phase === "transcribing" || phase === "formatting";

    let glyph = <Mic size={12} strokeWidth={2.2} />;
    if (phase === "recording") glyph = <span className="sv-pill__dot" />;
    // An edit keeps its pen from the first word to the result, so it never reads as dictation.
    if (edit && (phase === "recording" || busy)) glyph = <PenLine size={12} strokeWidth={2.2} />;
    if (phase === "done") glyph = <Check size={13} strokeWidth={2.8} />;
    if (phase === "error") glyph = <TriangleAlert size={12} strokeWidth={2.4} />;

    let trailing: ReactNode = null;
    if (phase === "recording") trailing = <Timer startedAt={recordingSince} />;
    if (busy) {
        let label = phase === "transcribing" ? "Transcribing" : "Formatting";
        if (edit && phase === "formatting") label = "Editing";
        trailing = <span className="sv-pill__label">{label}</span>;
    }

    let body = <LevelBars live={phase === "recording"} shimmer={busy} />;
    if (phase === "done") {
        const words = state.words ?? 0;
        body = edit ? (
            <span className="sv-pill__result">
                <span>Edited</span>
            </span>
        ) : (
            <span className="sv-pill__result">
                <span>
                    {words} {words === 1 ? "word" : "words"}
                </span>
                {state.note && <span className="sv-pill__note">{state.note}</span>}
            </span>
        );
    }
    if (phase === "error") {
        body = (
            <span className="sv-pill__message" title={state.message}>
                {state.message ?? "Dictation failed"}
            </span>
        );
    }

    return (
        <div className="sv-overlay" aria-live="polite">
            <div className={`sv-pill is-${phase}`} role="status">
                <span className="sv-pill__glyph">{glyph}</span>
                {body}
                {trailing}
            </div>
        </div>
    );
}
