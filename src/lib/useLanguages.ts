// FilePath: src/lib/useLanguages.ts
// The language registry from the backend. It is compiled into the app and never changes while it
// runs, so it is fetched once and shared by every component that asks for it.
import { useEffect, useState } from "react";
import { api, errorMessage, type LanguageInfo } from "./api";

export interface LanguagesState {
    /** Null until the registry has loaded. */
    languages: LanguageInfo[] | null;
    loadError: string | null;
}

let cached: LanguageInfo[] | null = null;
let pending: Promise<LanguageInfo[]> | null = null;

function loadLanguages(): Promise<LanguageInfo[]> {
    pending ??= api.listLanguages().then(
        (languages) => {
            cached = languages;
            return languages;
        },
        (error: unknown) => {
            // Forget the failed request so the next component to mount asks again.
            pending = null;
            throw error;
        },
    );
    return pending;
}

export function useLanguages(): LanguagesState {
    const [state, setState] = useState<LanguagesState>({ languages: cached, loadError: null });

    useEffect(() => {
        if (cached !== null) return;
        let disposed = false;
        loadLanguages().then(
            (languages) => {
                if (!disposed) setState({ languages, loadError: null });
            },
            (error: unknown) => {
                if (!disposed) setState({ languages: null, loadError: errorMessage(error) });
            },
        );
        return () => {
            disposed = true;
        };
    }, []);

    return state;
}

/** The registry entry for a tag, or a bare entry named by the tag if it is unknown. */
export function languageOf(languages: LanguageInfo[], tag: string): LanguageInfo {
    return (
        languages.find((language) => language.tag === tag) ?? {
            tag,
            name: tag,
            script: null,
            primary: false,
            fallback: false,
        }
    );
}
