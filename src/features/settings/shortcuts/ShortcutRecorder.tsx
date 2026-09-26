// FilePath: src/features/settings/shortcuts/ShortcutRecorder.tsx
// Shows a configured global shortcut and records a replacement from the next key combination.
// Global shortcuts are suspended while recording so pressing the current combination does not
// start a dictation; they are always resumed afterwards, including on unmount and window blur.
// The webview does not reliably report the Fn key on its own, so it is offered as a button for
// the dictation shortcuts. The edit shortcut can be turned off instead; Fn is never allowed there.
import { useEffect, useRef, useState } from "react";
import { api, errorMessage, type SettingsPatch } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button, ShortcutKeys } from "../../../ui";
import { FN_KEY, captureKey } from "./accelerator";
import "./ShortcutRecorder.css";

export type ShortcutField = "holdShortcut" | "toggleShortcut" | "editShortcut";

const FIELDS: ShortcutField[] = ["holdShortcut", "toggleShortcut", "editShortcut"];

const FIELD_LABEL: Record<ShortcutField, string> = {
    holdShortcut: "Hold to speak",
    toggleShortcut: "Toggle to speak",
    editShortcut: "Hold to edit",
};

function patchFor(field: ShortcutField, accelerator: string): SettingsPatch {
    switch (field) {
        case "holdShortcut":
            return { holdShortcut: accelerator };
        case "toggleShortcut":
            return { toggleShortcut: accelerator };
        case "editShortcut":
            return { editShortcut: accelerator };
    }
}

export function ShortcutRecorder({ field }: { field: ShortcutField }) {
    const { settings, refresh } = useSettings();
    const [recording, setRecording] = useState(false);
    const [live, setLive] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [saving, setSaving] = useState(false);
    // Read through refs so a settings refresh mid-recording does not tear down the listeners
    // (which would resume global shortcuts while the user is still pressing keys).
    const contextRef = useRef({ settings, refresh });
    const finishRef = useRef<(accelerator: string | null) => void>(() => undefined);

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

    const save = async (accelerator: string) => {
        setError(null);
        setSaving(true);
        try {
            await api.updateSettings(patchFor(field, accelerator));
            await refresh();
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setSaving(false);
        }
    };

    useEffect(() => {
        if (!recording) return;
        let finished = false;

        const finish = async (accelerator: string | null) => {
            if (finished) return;
            finished = true;
            setRecording(false);
            try {
                const current = contextRef.current.settings;
                if (accelerator !== null && accelerator !== current[field]) {
                    const taken = FIELDS.find(
                        (other) => other !== field && current[other] === accelerator,
                    );
                    if (taken !== undefined) {
                        setError(`${accelerator} is already used for ${FIELD_LABEL[taken]}.`);
                        return;
                    }
                    setSaving(true);
                    await api.updateSettings(patchFor(field, accelerator));
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
        finishRef.current = (accelerator) => {
            void finish(accelerator);
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

    const off = settings[field] === "";
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
                ) : off ? (
                    <span className="sv-shortcut-rec__prompt">Off</span>
                ) : (
                    <ShortcutKeys accelerator={settings[field]} />
                )}
                {recording ? (
                    <>
                        {field !== "editShortcut" && (
                            <Button
                                size="sm"
                                variant="secondary"
                                onClick={() => {
                                    finishRef.current(FN_KEY);
                                }}
                            >
                                Use fn
                            </Button>
                        )}
                        <Button
                            size="sm"
                            variant="ghost"
                            onClick={() => {
                                finishRef.current(null);
                            }}
                        >
                            Cancel
                        </Button>
                    </>
                ) : (
                    <>
                        <Button
                            size="sm"
                            variant="secondary"
                            loading={saving}
                            onClick={() => {
                                void start();
                            }}
                        >
                            {off ? "Set" : "Change"}
                        </Button>
                        {field === "editShortcut" && !off && (
                            <Button
                                size="sm"
                                variant="ghost"
                                disabled={saving}
                                onClick={() => {
                                    void save("");
                                }}
                            >
                                Turn off
                            </Button>
                        )}
                    </>
                )}
            </div>
            {recording && !error && <p className="sv-shortcut-rec__hint">Esc to cancel</p>}
            {!recording && settings[field] === FN_KEY && (
                <p className="sv-shortcut-rec__note">
                    If fn opens emoji or switches input source, set System Settings → Keyboard →
                    “Press fn key to” → Do Nothing.
                </p>
            )}
            {error && (
                <p className="sv-shortcut-rec__error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}
