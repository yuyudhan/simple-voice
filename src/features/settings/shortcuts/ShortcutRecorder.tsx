// FilePath: src/features/settings/shortcuts/ShortcutRecorder.tsx
// Shows a configured global shortcut and records a replacement from the next key combination.
// Global shortcuts are suspended while recording so pressing the current combination does not
// start a dictation; they are always resumed afterwards, including on unmount and window blur.
import { useEffect, useRef, useState } from "react";
import { api, errorMessage } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button, ShortcutKeys } from "../../../ui";
import { captureKey } from "./accelerator";
import "./ShortcutRecorder.css";

export type ShortcutField = "holdShortcut" | "toggleShortcut";

const FIELD_LABEL: Record<ShortcutField, string> = {
    holdShortcut: "Hold to speak",
    toggleShortcut: "Toggle to speak",
};

export function ShortcutRecorder({ field }: { field: ShortcutField }) {
    const { settings, refresh } = useSettings();
    const [recording, setRecording] = useState(false);
    const [live, setLive] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);
    // Read through refs so a settings refresh mid-recording does not tear down the listeners
    // (which would resume global shortcuts while the user is still pressing keys).
    const contextRef = useRef({ settings, refresh });
    const cancelRef = useRef<() => void>(() => undefined);

    useEffect(() => {
        contextRef.current = { settings, refresh };
    }, [settings, refresh]);

    const start = async () => {
        setError(null);
        setLive("");
        try {
            await api.suspendShortcuts(true);
            setRecording(true);
        } catch (e) {
            setError(errorMessage(e));
        }
    };

    useEffect(() => {
        if (!recording) return;
        let finished = false;
        const other: ShortcutField = field === "holdShortcut" ? "toggleShortcut" : "holdShortcut";

        const finish = async (accelerator: string | null) => {
            if (finished) return;
            finished = true;
            setRecording(false);
            try {
                const current = contextRef.current.settings;
                if (accelerator !== null && accelerator !== current[field]) {
                    if (accelerator === current[other]) {
                        setError(`${accelerator} is already used for ${FIELD_LABEL[other]}.`);
                        return;
                    }
                    setSaving(true);
                    await api.updateSettings(
                        field === "holdShortcut"
                            ? { holdShortcut: accelerator }
                            : { toggleShortcut: accelerator },
                    );
                    await contextRef.current.refresh();
                }
            } catch (e) {
                setError(errorMessage(e));
            } finally {
                setSaving(false);
                setLive("");
                await api.suspendShortcuts(false).catch((e: unknown) => {
                    setError(errorMessage(e));
                });
            }
        };
        cancelRef.current = () => {
            void finish(null);
        };

        const onKeyDown = (event: KeyboardEvent) => {
            event.preventDefault();
            event.stopPropagation();
            if (event.repeat) return;
            if (event.code === "Escape") {
                void finish(null);
                return;
            }
            const result = captureKey(event);
            if (result.kind === "complete") {
                setError(null);
                setLive(result.accelerator);
                void finish(result.accelerator);
            } else {
                setLive(result.modifiers.join("+"));
                setError(result.kind === "invalid" ? result.reason : null);
            }
        };
        const onKeyUp = (event: KeyboardEvent) => {
            event.preventDefault();
            event.stopPropagation();
            const result = captureKey(event);
            if (result.kind === "partial") setLive(result.modifiers.join("+"));
        };
        const onBlur = () => {
            void finish(null);
        };

        window.addEventListener("keydown", onKeyDown, true);
        window.addEventListener("keyup", onKeyUp, true);
        window.addEventListener("blur", onBlur);
        return () => {
            window.removeEventListener("keydown", onKeyDown, true);
            window.removeEventListener("keyup", onKeyUp, true);
            window.removeEventListener("blur", onBlur);
            if (!finished) {
                finished = true;
                void api.suspendShortcuts(false).catch(() => undefined);
            }
        };
    }, [recording, field]);

    return (
        <div className="sv-shortcut-rec">
            <div className="sv-shortcut-rec__row">
                {recording ? (
                    <div className="sv-shortcut-rec__capture" aria-live="polite">
                        {live ? (
                            <ShortcutKeys accelerator={live} />
                        ) : (
                            <span className="sv-shortcut-rec__prompt">Press a key combination</span>
                        )}
                    </div>
                ) : (
                    <ShortcutKeys accelerator={settings[field]} />
                )}
                {recording ? (
                    <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => {
                            cancelRef.current();
                        }}
                    >
                        Cancel
                    </Button>
                ) : (
                    <Button
                        size="sm"
                        variant="secondary"
                        loading={saving}
                        onClick={() => {
                            void start();
                        }}
                    >
                        Change
                    </Button>
                )}
            </div>
            {recording && !error && <p className="sv-shortcut-rec__hint">Esc to cancel</p>}
            {error && (
                <p className="sv-shortcut-rec__error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}
