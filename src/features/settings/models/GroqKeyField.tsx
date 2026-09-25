// FilePath: src/features/settings/models/GroqKeyField.tsx
// Groq API key entry. A key is verified against Groq before it is stored so a typo surfaces
// here instead of as a failed dictation later. One key serves transcription and formatting.
import { useState, type SyntheticEvent } from "react";
import { CircleCheck, ExternalLink, Eye, EyeOff } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorMessage } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button, IconButton, TextField } from "../../../ui";
import "../common.css";
import "./models.css";

const GROQ_KEYS_URL = "https://console.groq.com/keys";

export function GroqKeyField({ onSaved }: { onSaved?: () => void }) {
    const { settings, refresh } = useSettings();
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
            await api.verifyGroqApiKey(trimmed);
            await api.setGroqApiKey(trimmed);
            await refresh();
            setKey("");
            setReplacing(false);
            onSaved?.();
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
            await api.setGroqApiKey(null);
            await refresh();
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setBusy(null);
        }
    };

    const showForm = !settings.groqApiKeyPresent || replacing;

    return (
        <div className="sv-keycard">
            <div className="sv-keycard__head">
                <div>
                    <p className="sv-keycard__title">Groq API key</p>
                    <p className="sv-keycard__desc">
                        One key serves Groq Whisper transcription and Groq formatting. It is stored
                        only in your local database.
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
            {showForm ? (
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
            ) : (
                <div className="sv-keycard__saved">
                    <CircleCheck size={16} />
                    <span>Key saved</span>
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
            {!showForm && error && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}
