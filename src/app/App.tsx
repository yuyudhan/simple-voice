// FilePath: src/app/App.tsx
import { useCallback, useMemo, useState } from "react";
import { events, type NavigateTarget } from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import { Onboarding } from "../features/onboarding/Onboarding";
import { SettingsDialog } from "../features/settings/SettingsDialog";
import { HistoryPage } from "../features/history/HistoryPage";
import { InsightsPage } from "../features/insights/InsightsPage";
import { DictionaryPage } from "../features/dictionary/DictionaryPage";
import { StylePage } from "../features/style/StylePage";
import { useSettings } from "./SettingsContext";
import { ShellContext, type Page, type SettingsSection } from "./ShellContext";
import { Sidebar } from "./Sidebar";
import "./App.css";

export function App() {
    const { settings, refresh } = useSettings();
    const [page, setPage] = useState<Page>("home");
    const [dialog, setDialog] = useState<{ open: boolean; section?: SettingsSection }>({
        open: false,
    });

    const openSettings = useCallback((section?: SettingsSection) => {
        setDialog({ open: true, section });
    }, []);

    const shell = useMemo(() => ({ openSettings, navigate: setPage }), [openSettings]);

    // Exhaustive per-target handlers: adding a NavigateTarget member fails to type-check here.
    useTauriEvent(events.navigate, (target) => {
        const handlers: Record<NavigateTarget, () => void> = { settings: openSettings };
        handlers[target]();
    });

    if (!settings.onboardingComplete) {
        return (
            <Onboarding
                settings={settings}
                onDone={() => {
                    void refresh();
                }}
            />
        );
    }

    return (
        <ShellContext.Provider value={shell}>
            <div className="sv-shell">
                <div className="sv-shell__drag" data-tauri-drag-region />
                <Sidebar page={page} onNavigate={setPage} onOpenSettings={openSettings} />
                <main className="sv-shell__main">
                    <div className="sv-shell__panel">
                        <div className="sv-shell__content" key={page}>
                            {page === "home" && <HistoryPage />}
                            {page === "insights" && <InsightsPage />}
                            {page === "dictionary" && <DictionaryPage />}
                            {page === "style" && <StylePage />}
                        </div>
                    </div>
                </main>
            </div>
            <SettingsDialog
                open={dialog.open}
                initialSection={dialog.section}
                onClose={() => {
                    setDialog((current) => ({ ...current, open: false }));
                }}
            />
        </ShellContext.Provider>
    );
}
