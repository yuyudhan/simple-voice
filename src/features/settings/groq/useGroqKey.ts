// FilePath: src/features/settings/groq/useGroqKey.ts
// Whether a Groq API key is saved (the key itself stays in the Keychain, behind the backend).
// Every mounted copy of the hook hears a save or removal made through any other copy, so the
// key card and the provider notes on one page never disagree.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage } from "../../../lib/api";

const CHANGED = "sv-groq-key-changed";

export interface GroqKeyState {
    /** `null` until the first answer. */
    saved: boolean | null;
    /** Why the status could not be read. */
    error: string | null;
    /** Verifies the key with Groq, then saves it; rejects with Groq's reason. */
    save: (key: string) => Promise<void>;
    remove: () => Promise<void>;
}

function announce(saved: boolean) {
    window.dispatchEvent(new CustomEvent<boolean>(CHANGED, { detail: saved }));
}

export function useGroqKey(): GroqKeyState {
    const [saved, setSaved] = useState<boolean | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let current = true;
        api.groqKeyStatus().then(
            (status) => {
                if (current) setSaved(status.saved);
            },
            (e: unknown) => {
                if (current) setError(errorMessage(e));
            },
        );
        const onChanged = (event: Event) => {
            if (event instanceof CustomEvent && typeof event.detail === "boolean") {
                setSaved(event.detail);
                setError(null);
            }
        };
        window.addEventListener(CHANGED, onChanged);
        return () => {
            current = false;
            window.removeEventListener(CHANGED, onChanged);
        };
    }, []);

    const save = useCallback(async (key: string) => {
        const status = await api.saveGroqApiKey(key);
        announce(status.saved);
    }, []);

    const remove = useCallback(async () => {
        const status = await api.removeGroqApiKey();
        announce(status.saved);
    }, []);

    return { saved, error, save, remove };
}
