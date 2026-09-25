// FilePath: src/features/onboarding/ShortcutStep.tsx
// Shortcut setup plus a practice box. Focusing the textarea makes Simple Voice the frontmost app,
// so a real dictation pastes straight into it.
import { useCallback, useState } from "react";
import { Command } from "lucide-react";
import { events, type DictationState } from "../../lib/api";
import { useTauriEvent } from "../../lib/useTauriEvent";
import { useSettings } from "../../app/SettingsContext";
import { Button, ShortcutKeys } from "../../ui";
import { FnKeyAccessWarning } from "../settings/shortcuts/FnKeyAccessWarning";
import { ShortcutRecorder } from "../settings/shortcuts/ShortcutRecorder";
import { StepFooter } from "./StepFooter";

function describe(state: DictationState): { text: string; tone: string } {
    switch (state.phase) {
        case "idle":
            return { text: "Ready", tone: "idle" };
        case "recording":
            return { text: "Listening…", tone: "live" };
        case "transcribing":
            return { text: "Transcribing…", tone: "busy" };
        case "formatting":
            return { text: "Formatting…", tone: "busy" };
        case "done": {
            const words = state.words ?? 0;
            const note = state.note ? ` · ${state.note}` : "";
            const count = `${String(words)} ${words === 1 ? "word" : "words"}`;
            return { text: `Done · ${count}${note}`, tone: "done" };
        }
        case "error":
            return { text: state.message ?? "Something went wrong", tone: "error" };
        case "cancelled":
            return { text: "Cancelled", tone: "idle" };
    }
}

interface Props {
    onBack: () => void;
    onNext: () => void;
}

export function ShortcutStep({ onBack, onNext }: Props) {
    const { settings } = useSettings();
    const [state, setState] = useState<DictationState>({ phase: "idle", sessionId: 0 });
    const [text, setText] = useState("");
    const onState = useCallback((next: DictationState) => {
        setState(next);
    }, []);
    useTauriEvent(events.dictationState, onState);
    const status = describe(state);

    return (
        <>
            <div className="sv-onb__icon">
                <Command size={20} />
            </div>
            <h1 className="sv-onb__title">Your shortcuts</h1>
            <p className="sv-onb__lead">
                Hold the first to talk and let go to paste. Use the second for longer thoughts:
                press once to start and again to stop. Esc cancels.
            </p>
            <div className="sv-onb__wide">
                <div className="sv-onb__shortcuts">
                    <div className="sv-onb__shortcut">
                        <div>
                            <p className="sv-onb__shortcut-title">Hold to speak</p>
                            <p className="sv-inline-note">Recording stops when you release.</p>
                        </div>
                        <ShortcutRecorder field="holdShortcut" />
                    </div>
                    <div className="sv-onb__shortcut">
                        <div>
                            <p className="sv-onb__shortcut-title">Toggle to speak</p>
                            <p className="sv-inline-note">Press to start, press again to stop.</p>
                        </div>
                        <ShortcutRecorder field="toggleShortcut" />
                    </div>
                </div>
                <FnKeyAccessWarning placement="inline" />

                <div className="sv-onb__try">
                    <div className="sv-onb__try-head">
                        <span className="caps-label">Try it</span>
                        <span className={`sv-onb__state is-${status.tone}`} aria-live="polite">
                            <span className="sv-onb__state-dot" />
                            {status.text}
                        </span>
                    </div>
                    <textarea
                        className="sv-onb__textarea"
                        value={text}
                        rows={4}
                        placeholder="Click here, hold your shortcut, say something, and let go."
                        onChange={(event) => {
                            setText(event.target.value);
                        }}
                    />
                    <div className="sv-inline-note sv-onb__try-hint">
                        Hold <ShortcutKeys accelerator={settings.holdShortcut} /> and speak.
                    </div>
                </div>
            </div>
            <StepFooter
                onBack={onBack}
                next={
                    <Button variant="primary" onClick={onNext}>
                        Continue
                    </Button>
                }
            />
        </>
    );
}
