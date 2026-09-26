// FilePath: src/app/Sidebar.tsx
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Activity as Pulse, AudioLines, BookA, Settings as Gear, Type } from "lucide-react";
import {
    api,
    errorMessage,
    events,
    type DictationPhase,
    type Permissions,
    type Settings,
} from "../lib/api";
import { useTauriEvent } from "../lib/useTauriEvent";
import { SETTINGS_PAGES } from "../features/settings/settingsPages";
import { BrandMark, IconButton, ShortcutKeys, useToast } from "../ui";
import { useSettings } from "./SettingsContext";
import type { Page } from "./ShellContext";
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
    { id: "home", label: "History", icon: <AudioLines /> },
    { id: "insights", label: "Insights", icon: <Pulse /> },
    { id: "dictionary", label: "Dictionary", icon: <BookA /> },
    { id: "style", label: "Style", icon: <Type /> },
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
}

interface NavItemProps {
    id: Page;
    label: string;
    icon: ReactNode;
    active: boolean;
    alert?: boolean;
    onNavigate: (page: Page) => void;
}

function NavItem({ id, label, icon, active, alert = false, onNavigate }: NavItemProps) {
    return (
        <button
            type="button"
            className={`sv-nav__item${active ? " is-active" : ""}`}
            aria-current={active ? "page" : undefined}
            aria-label={alert ? `${label}, a permission is missing` : undefined}
            onClick={() => {
                onNavigate(id);
            }}
        >
            <span className="sv-nav__icon" aria-hidden="true">
                {icon}
                {alert && <span className="sv-nav__alert" />}
            </span>
            {label}
        </button>
    );
}

export function Sidebar({ page, onNavigate }: SidebarProps) {
    const { settings } = useSettings();
    const { toast } = useToast();
    const [activity, setActivity] = useState<Activity>("idle");
    const [permissions, setPermissions] = useState<Permissions | null>(null);
    const [version, setVersion] = useState<string | null>(null);

    useEffect(() => {
        void api.appInfo().then(
            (info) => {
                setVersion(info.version);
            },
            (error: unknown) => {
                console.error("app_info failed", error);
            },
        );
    }, []);

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
            <div className="sv-sidebar__top" data-tauri-drag-region />

            <div className="sv-brand" data-tauri-drag-region>
                <BrandMark className="sv-brand__mark" />
                <span className="sv-brand__name">Simple Voice</span>
                {version && <span className="sv-brand__version">v{version}</span>}
            </div>

            <nav className="sv-nav" aria-label="Main">
                {NAV.map((item) => (
                    <NavItem
                        key={item.id}
                        {...item}
                        active={page === item.id}
                        onNavigate={onNavigate}
                    />
                ))}
            </nav>

            <div className="sv-sidebar__bottom">
                <nav className="sv-nav" aria-labelledby="sv-nav-settings">
                    <span id="sv-nav-settings" className="sv-nav__label caps-label">
                        Settings
                    </span>
                    {SETTINGS_PAGES.map((item) => (
                        <NavItem
                            key={item.id}
                            id={item.id}
                            label={item.label}
                            icon={item.icon}
                            active={page === item.id}
                            alert={item.id === "permissions" && needsPermission}
                            onNavigate={onNavigate}
                        />
                    ))}
                </nav>
                <div className={`sv-status sv-status--${activity}`}>
                    <IconButton
                        className="sv-status__settings"
                        label="Shortcut settings"
                        icon={<Gear />}
                        onClick={() => {
                            onNavigate("general");
                        }}
                    />
                    <span className="sv-status__line" role="status" aria-live="polite">
                        <span className="sv-status__dot" aria-hidden="true" />
                        {ACTIVITY_LABEL[activity]}
                    </span>
                    <span className="sv-status__hint">
                        Hold <ShortcutKeys accelerator={settings.holdShortcut} /> to speak
                    </span>
                    <span className="sv-status__hint">
                        Press <ShortcutKeys accelerator={settings.toggleShortcut} /> to toggle
                    </span>
                </div>
            </div>
        </aside>
    );
}
