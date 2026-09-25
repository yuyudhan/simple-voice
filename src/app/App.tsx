// FilePath: src/app/App.tsx
import { useCallback, useMemo, useState } from "react";
import { events, type NavigateTarget } from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import { Onboarding } from "../features/onboarding/Onboarding";
import { SettingsPage } from "../features/settings/SettingsPage";
import { HistoryPage } from "../features/history/HistoryPage";
import { InsightsPage } from "../features/insights/InsightsPage";
import { DictionaryPage } from "../features/dictionary/DictionaryPage";
import { StylePage } from "../features/style/StylePage";
import { AccessibilityWarning } from "../features/settings/permissions/AccessibilityWarning";
import { UpdateBanner } from "../features/updates/UpdateBanner";
import { useToast } from "../ui";
import { useSettings } from "./SettingsContext";
import { isSettingsSection, ShellContext, type Page } from "./ShellContext";
import { Sidebar } from "./Sidebar";
import { useTheme } from "./theme";
import { ThemeToggle } from "./ThemeToggle";
import "./App.css";

export function App() {
    const { settings, refresh } = useSettings();
    const { toast } = useToast();
    const themeError = useCallback(
        (message: string) => {
            toast(`Could not apply the theme to the window: ${message}`, "danger");
        },
        [toast],
    );
    const resolvedTheme = useTheme(settings.theme, themeError);
    const [page, setPage] = useState<Page>("home");

    const shell = useMemo(() => ({ navigate: setPage }), []);

    // Exhaustive per-target handlers: adding a NavigateTarget member fails to type-check here.
    useTauriEvent(events.navigate, (target) => {
        const handlers: Record<NavigateTarget, () => void> = {
            settings: () => {
                setPage("general");
            },
            updates: () => {
                setPage("system");
            },
        };
        handlers[target]();
    });

    if (!settings.onboardingComplete) {
        return (
            <>
                <Onboarding
                    settings={settings}
                    onDone={() => {
                        void refresh();
                    }}
                />
                <ThemeToggle resolved={resolvedTheme} />
            </>
        );
    }

    return (
        <ShellContext.Provider value={shell}>
            <div className="sv-shell">
                <div className="sv-shell__drag" data-tauri-drag-region />
                <ThemeToggle resolved={resolvedTheme} />
                <Sidebar page={page} onNavigate={setPage} />
                <main className="sv-shell__main">
                    <div className="sv-shell__notices">
                        <AccessibilityWarning placement="shell" />
                        <UpdateBanner />
                    </div>
                    <div className="sv-shell__content" key={page}>
                        {page === "home" && <HistoryPage />}
                        {page === "insights" && <InsightsPage />}
                        {page === "dictionary" && <DictionaryPage />}
                        {page === "style" && <StylePage />}
                        {isSettingsSection(page) && <SettingsPage section={page} />}
                    </div>
                </main>
            </div>
        </ShellContext.Provider>
    );
}
