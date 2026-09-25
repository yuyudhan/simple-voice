// FilePath: src/app/Sidebar.tsx
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { BookA, ChartColumn, Mic, PenLine, Settings as SettingsIcon } from "lucide-react";
import {
    api,
    errorMessage,
    events,
    type DictationPhase,
    type Permissions,
    type Settings,
} from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import { ShortcutKeys, useToast } from "../ui";
import { useSettings } from "./SettingsContext";
import type { Page, SettingsSection } from "./ShellContext";
import "./Sidebar.css";

type Activity = "idle" | "recording" | "processing";

const ACTIVITY_LABEL: Record<Activity, string> = {
    idle: "Ready",
    recording: "Listening",
    processing: "Transcribing",
};

const PHASE_ACTIVITY: Record<DictationPhase, Activity> = {
    idle: "idle",
    recording: "recording",
    transcribing: "processing",
    formatting: "processing",
    done: "idle",
    error: "idle",
    cancelled: "idle",
};

const NAV: { id: Page; label: string; icon: ReactNode }[] = [
    { id: "home", label: "Home", icon: <Mic /> },
    { id: "insights", label: "Insights", icon: <ChartColumn /> },
    { id: "dictionary", label: "Dictionary", icon: <BookA /> },
    { id: "style", label: "Style", icon: <PenLine /> },
];

/** Apple Speech needs the Speech Recognition grant; every setup needs mic + accessibility. */
function missingPermission(permissions: Permissions, settings: Settings): boolean {
    return (
        permissions.microphone !== "granted" ||
        permissions.accessibility !== "granted" ||
        (settings.transcriptionModel === "apple-speech" && permissions.speech !== "granted")
    );
}

export interface SidebarProps {
    page: Page;
    onNavigate: (page: Page) => void;
    onOpenSettings: (section?: SettingsSection) => void;
}

export function Sidebar({ page, onNavigate, onOpenSettings }: SidebarProps) {
    const { settings } = useSettings();
    const { toast } = useToast();
    const [activity, setActivity] = useState<Activity>("idle");
    const [permissions, setPermissions] = useState<Permissions | null>(null);

    const loadPermissions = useCallback(
        () =>
            api.getPermissions().then(setPermissions, (error: unknown) => {
                toast(errorMessage(error), "danger");
            }),
        [toast],
    );

    useEffect(() => {
        void loadPermissions();
        // Permissions change in System Settings while the app is in the background; re-check
        // whenever the window regains focus.
        const onFocus = () => {
            void loadPermissions();
        };
        window.addEventListener("focus", onFocus);
        return () => {
            window.removeEventListener("focus", onFocus);
        };
    }, [loadPermissions]);

    useTauriEvent(events.permissionsChanged, setPermissions);
    useTauriEvent(events.dictationState, (state) => {
        setActivity(PHASE_ACTIVITY[state.phase]);
    });

    const needsPermission = permissions !== null && missingPermission(permissions, settings);

    return (
        <aside className="sv-sidebar">
            <div className="sv-sidebar__top" data-tauri-drag-region>
                <span
                    className={`sv-status sv-status--${activity}`}
                    role="status"
                    aria-live="polite"
                    data-tauri-drag-region
                >
                    <span className="sv-status__dot" aria-hidden="true" />
                    {ACTIVITY_LABEL[activity]}
                </span>
            </div>

            <div className="sv-brand" data-tauri-drag-region>
                <svg className="sv-brand__mark" viewBox="0 0 20 20" aria-hidden="true">
                    <rect x="2" y="7.5" width="2.4" height="5" rx="1.2" />
                    <rect x="6.2" y="4.5" width="2.4" height="11" rx="1.2" />
                    <rect x="10.4" y="2" width="2.4" height="16" rx="1.2" />
                    <rect x="14.6" y="6" width="2.4" height="8" rx="1.2" />
                </svg>
                <span className="sv-brand__name">Simple Voice</span>
            </div>

            <nav className="sv-nav" aria-label="Main">
                {NAV.map((item) => (
                    <button
                        key={item.id}
                        type="button"
                        className={`sv-nav__item${page === item.id ? " is-active" : ""}`}
                        aria-current={page === item.id ? "page" : undefined}
                        onClick={() => {
                            onNavigate(item.id);
                        }}
                    >
                        <span className="sv-nav__icon" aria-hidden="true">
                            {item.icon}
                        </span>
                        {item.label}
                    </button>
                ))}
            </nav>

            <div className="sv-sidebar__bottom">
                <button
                    type="button"
                    className="sv-nav__item"
                    aria-label={needsPermission ? "Settings, a permission is missing" : "Settings"}
                    onClick={() => {
                        onOpenSettings(needsPermission ? "permissions" : undefined);
                    }}
                >
                    <span className="sv-nav__icon" aria-hidden="true">
                        <SettingsIcon />
                        {needsPermission && <span className="sv-nav__alert" />}
                    </span>
                    Settings
                </button>
                <div className="sv-hint">
                    Hold <ShortcutKeys accelerator={settings.holdShortcut} /> to speak
                </div>
            </div>
        </aside>
    );
}
