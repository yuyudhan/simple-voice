// FilePath: src/features/settings/groq/GroqKeyField.tsx
// Groq API key entry and the privacy note every Groq choice shows. A key is verified with Groq
// before it is saved to the macOS Keychain, so a typo surfaces here instead of as a failed
// dictation later. One key serves Groq transcription, formatting and meetings.
import { useState, type ReactNode, type SyntheticEvent } from "react";
import { CircleCheck, CloudUpload, ExternalLink, Eye, EyeOff } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { errorMessage } from "../../../lib/api";
import { Button, IconButton, TextField } from "../../../ui";
import type { GroqKeyState } from "./useGroqKey";
import "../common.css";
import "./groq.css";

const GROQ_KEYS_URL = "https://console.groq.com/keys";

/** States plainly what leaves the Mac when a Groq provider is chosen. */
export function CloudNotice({ children }: { children: ReactNode }) {
    return (
        <div className="sv-cloud-notice" role="note">
            <CloudUpload size={16} aria-hidden="true" />
            <p>{children}</p>
        </div>
    );
}

export function GroqKeyField({ groq }: { groq: GroqKeyState }) {
    const [key, setKey] = useState("");
    const [reveal, setReveal] = useState(false);
    const [replacing, setReplacing] = useState(false);
    const [busy, setBusy] = useState<"save" | "remove" | null>(null);
    const [error, setError] = useState<string | null>(null);

    const save = async (event: SyntheticEvent) => {
        event.preventDefault();
        const trimmed = key.trim();
        if (trimmed === "") {
            setError("Paste your Groq API key first.");
            return;
        }
        setBusy("save");
        setError(null);
        try {
            await groq.save(trimmed);
            setKey("");
            setReplacing(false);
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setBusy(null);
        }
    };

    const remove = async () => {
        setBusy("remove");
        setError(null);
        try {
            await groq.remove();
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setBusy(null);
        }
    };

    const showForm = groq.saved === false || replacing;
    const shownError = error ?? groq.error;

    return (
        <div className="sv-keycard">
            <div className="sv-keycard__head">
                <div>
                    <p className="sv-keycard__title">Groq API key</p>
                    <p className="sv-keycard__desc">
                        Your own key from Groq. It is checked with Groq, then kept in the macOS
                        Keychain, never in Simple Voice's database.
                    </p>
                </div>
                <button
                    type="button"
                    className="sv-link"
                    onClick={() => {
                        void openUrl(GROQ_KEYS_URL).catch((e: unknown) => {
                            setError(errorMessage(e));
                        });
                    }}
                >
                    Get a key <ExternalLink size={12} />
                </button>
            </div>
            {groq.saved === null && !shownError && <p className="sv-inline-note">Checking…</p>}
            {showForm && (
                <form
                    className="sv-keycard__form"
                    onSubmit={(event) => {
                        void save(event);
                    }}
                >
                    <TextField
                        type={reveal ? "text" : "password"}
                        value={key}
                        placeholder="gsk_…"
                        autoComplete="off"
                        spellCheck={false}
                        aria-label="Groq API key"
                        error={error ?? undefined}
                        onChange={(event) => {
                            setKey(event.target.value);
                        }}
                    />
                    <IconButton
                        type="button"
                        label={reveal ? "Hide key" : "Show key"}
                        icon={reveal ? <EyeOff size={15} /> : <Eye size={15} />}
                        onClick={() => {
                            setReveal((r) => !r);
                        }}
                    />
                    <Button type="submit" variant="primary" loading={busy === "save"}>
                        {busy === "save" ? "Verifying" : "Save"}
                    </Button>
                    {replacing && (
                        <Button
                            type="button"
                            variant="ghost"
                            onClick={() => {
                                setReplacing(false);
                                setKey("");
                                setError(null);
                            }}
                        >
                            Cancel
                        </Button>
                    )}
                </form>
            )}
            {groq.saved === true && !replacing && (
                <div className="sv-keycard__saved">
                    <CircleCheck size={16} />
                    <span>Key saved in the Keychain</span>
                    <div className="sv-keycard__actions">
                        <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => {
                                setReplacing(true);
                            }}
                        >
                            Replace
                        </Button>
                        <Button
                            size="sm"
                            variant="ghost"
                            loading={busy === "remove"}
                            onClick={() => {
                                void remove();
                            }}
                        >
                            Remove
                        </Button>
                    </div>
                </div>
            )}
            {!showForm && shownError && (
                <p className="sv-inline-error" role="alert">
                    {shownError}
                </p>
            )}
        </div>
    );
}
