// FilePath: src/app/SettingsContext.tsx
import {
    createContext,
    useCallback,
    useContext,
    useEffect,
    useMemo,
    useState,
    type ReactNode,
} from "react";
import { api, errorMessage, events, type Settings, type SettingsPatch } from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import { Button, Spinner, useToast } from "../ui";
import "./SettingsContext.css";

export interface SettingsContextValue {
    settings: Settings;
    /** Applies a patch; resolves false (after showing a toast) when the backend rejects it. */
    update: (patch: SettingsPatch) => Promise<boolean>;
    refresh: () => Promise<void>;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

type LoadState =
    | { kind: "loading" }
    | { kind: "ready"; settings: Settings }
    | { kind: "error"; message: string };

export function SettingsProvider({ children }: { children: ReactNode }) {
    const [state, setState] = useState<LoadState>({ kind: "loading" });
    const { toast } = useToast();

    // State is only set from promise callbacks, so calling this from an effect never updates
    // state synchronously.
    const refresh = useCallback(
        () =>
            api.getSettings().then(
                (settings) => {
                    setState({ kind: "ready", settings });
                },
                (error: unknown) => {
                    const message = errorMessage(error);
                    setState((current) =>
                        current.kind === "ready" ? current : { kind: "error", message },
                    );
                    toast(message, "danger");
                },
            ),
        [toast],
    );

    useEffect(() => {
        void refresh();
    }, [refresh]);

    useTauriEvent(events.settingsChanged, (settings) => {
        setState({ kind: "ready", settings });
    });

    const update = useCallback(
        async (patch: SettingsPatch) => {
            try {
                const settings = await api.updateSettings(patch);
                setState({ kind: "ready", settings });
                return true;
            } catch (error) {
                toast(errorMessage(error), "danger");
                return false;
            }
        },
        [toast],
    );

    const value = useMemo(
        () => (state.kind === "ready" ? { settings: state.settings, update, refresh } : null),
        [state, update, refresh],
    );

    if (value) {
        return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
    }
    if (state.kind === "error") {
        return (
            <div className="sv-boot" role="alert">
                <h1 className="sv-boot__title">Simple Voice could not start</h1>
                <p className="sv-boot__message selectable">{state.message}</p>
                <Button
                    variant="primary"
                    onClick={() => {
                        setState({ kind: "loading" });
                        void refresh();
                    }}
                >
                    Try again
                </Button>
            </div>
        );
    }
    return (
        <div className="sv-boot" data-tauri-drag-region>
            <Spinner size={20} />
        </div>
    );
}

export function useSettings(): SettingsContextValue {
    const context = useContext(SettingsContext);
    if (!context) throw new Error("useSettings must be used inside <SettingsProvider>");
    return context;
}
