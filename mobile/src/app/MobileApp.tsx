// FilePath: mobile/src/app/MobileApp.tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { Activity, AudioLines, BookA, Settings as Gear, type LucideIcon } from "lucide-react";
import { useSettings } from "../../../src/app/SettingsContext";
import { ShellContext, type Page } from "../../../src/app/ShellContext";
import { DictionaryPage } from "../../../src/features/dictionary/DictionaryPage";
import { HistoryPage } from "../../../src/features/history/HistoryPage";
import { InsightsPage } from "../../../src/features/insights/InsightsPage";
import { MobileSettings } from "../features/settings/MobileSettings";
import { SetupGuide } from "../features/setup/SetupGuide";
import { useKeyboardRequest } from "../features/setup/useKeyboardRequest";
import { useMobileTheme } from "./useMobileTheme";

type Tab = "home" | "insights" | "dictionary" | "settings";

// Same icons as the desktop sidebar, so both apps read the same.
const TABS: { id: Tab; label: string; icon: LucideIcon }[] = [
    { id: "home", label: "History", icon: AudioLines },
    { id: "insights", label: "Insights", icon: Activity },
    { id: "dictionary", label: "Dictionary", icon: BookA },
    { id: "settings", label: "Settings", icon: Gear },
];

/** Desktop pages navigate by page name; on a phone every settings page lives in one tab. */
function tabFor(page: Page): Tab {
    if (page === "home" || page === "insights" || page === "dictionary") return page;
    return "settings";
}

export function MobileApp() {
    const { settings } = useSettings();
    useMobileTheme(settings.theme);
    const [tab, setTab] = useState<Tab>("home");
    const shell = useMemo(
        () => ({
            navigate: (page: Page) => {
                setTab(tabFor(page));
            },
        }),
        [],
    );
    const openSettings = useCallback(() => {
        setTab("settings");
    }, []);
    const [request, setRequest] = useKeyboardRequest(openSettings);
    // First launch shows the setup guide until it is finished or skipped; Settings reopens it.
    const [guide, setGuide] = useState(false);
    const closeGuide = useCallback(() => {
        setGuide(false);
    }, []);
    const openGuide = useCallback(() => {
        setGuide(true);
    }, []);
    // The keyboard writes history while the app sits in the background, without Tauri events;
    // coming back to the app reloads the open tab so it shows those dictations.
    const [visits, setVisits] = useState(0);
    useEffect(() => {
        const onVisible = () => {
            if (document.visibilityState === "visible") setVisits((count) => count + 1);
        };
        document.addEventListener("visibilitychange", onVisible);
        return () => {
            document.removeEventListener("visibilitychange", onVisible);
        };
    }, []);

    if (guide || !settings.onboardingComplete) {
        return <SetupGuide request={request} onRequest={setRequest} onClose={closeGuide} />;
    }

    return (
        <ShellContext.Provider value={shell}>
            <div className="svm-shell">
                <main className="svm-shell__content" key={`${tab}-${String(visits)}`}>
                    {tab === "home" && <HistoryPage />}
                    {tab === "insights" && <InsightsPage />}
                    {tab === "dictionary" && <DictionaryPage />}
                    {tab === "settings" && (
                        <MobileSettings
                            request={request}
                            onRequest={setRequest}
                            onOpenGuide={openGuide}
                        />
                    )}
                </main>
                <nav className="svm-tabs" aria-label="Sections">
                    {TABS.map(({ id, label, icon: Icon }) => (
                        <button
                            key={id}
                            type="button"
                            className="svm-tabs__tab"
                            aria-current={tab === id ? "page" : undefined}
                            onClick={() => {
                                setTab(id);
                            }}
                        >
                            <Icon aria-hidden="true" />
                            <span>{label}</span>
                        </button>
                    ))}
                </nav>
            </div>
        </ShellContext.Provider>
    );
}
