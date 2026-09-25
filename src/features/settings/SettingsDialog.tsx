// FilePath: src/features/settings/SettingsDialog.tsx
// Settings sheet: a left navigation column and a right pane with the selected section. It is a
// custom sheet rather than the generic Modal because it needs a full-bleed two-column layout;
// confirmations inside sections still use Modal and stack above it.
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { AudioLines, Database, Monitor, ShieldCheck, SlidersHorizontal, X } from "lucide-react";
import { api, type AppInfo } from "../../lib/api";
import type { SettingsSection } from "../../app/ShellContext";
import { IconButton } from "../../ui";
import { DataSection } from "./data/DataSection";
import { GeneralSection } from "./general/GeneralSection";
import { ModelsSection } from "./models/ModelsSection";
import { PermissionsSection } from "./permissions/PermissionsSection";
import { SystemSection } from "./system/SystemSection";
import "./common.css";
import "./SettingsDialog.css";

const NAV: { id: SettingsSection; label: string; description: string; icon: ReactNode }[] = [
    {
        id: "general",
        label: "General",
        description: "Appearance, shortcuts, microphone and dictation languages.",
        icon: <SlidersHorizontal size={16} />,
    },
    {
        id: "system",
        label: "System",
        description: "Startup, sound cues and how dictated text is delivered.",
        icon: <Monitor size={16} />,
    },
    {
        id: "models",
        label: "Models",
        description: "The voice model that transcribes and the provider that formats.",
        icon: <AudioLines size={16} />,
    },
    {
        id: "permissions",
        label: "Permissions",
        description: "What macOS allows Simple Voice to do.",
        icon: <ShieldCheck size={16} />,
    },
    {
        id: "data",
        label: "Data",
        description: "Where your history is stored, and how to clear it.",
        icon: <Database size={16} />,
    },
];

interface Props {
    open: boolean;
    onClose: () => void;
    initialSection?: SettingsSection;
}

function SettingsSheet({ onClose, initialSection }: Omit<Props, "open">) {
    const [section, setSection] = useState<SettingsSection>(initialSection ?? "general");
    const [info, setInfo] = useState<AppInfo | null>(null);
    const sheetRef = useRef<HTMLDivElement>(null);

    const loadInfo = useCallback(
        () =>
            api.appInfo().then(setInfo, (e: unknown) => {
                console.error("app_info failed", e);
            }),
        [],
    );

    useEffect(() => {
        void loadInfo();
    }, [loadInfo]);

    // The parent passes a fresh onClose on every render (any settings change re-renders it);
    // reading it through a ref keeps this effect, and its initial focus, to mount only, so
    // keyboard users keep their focus while they change settings.
    const onCloseRef = useRef(onClose);
    useEffect(() => {
        onCloseRef.current = onClose;
    }, [onClose]);

    useEffect(() => {
        sheetRef.current?.focus();
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== "Escape" || event.defaultPrevented) return;
            // A confirmation dialog stacked above the sheet handles its own Escape.
            const dialogs = document.querySelectorAll('[role="dialog"]');
            if (dialogs[dialogs.length - 1] !== sheetRef.current) return;
            event.preventDefault();
            onCloseRef.current();
        };
        document.addEventListener("keydown", onKeyDown);
        return () => {
            document.removeEventListener("keydown", onKeyDown);
        };
    }, []);

    const current = NAV.find((item) => item.id === section) ?? NAV[0];

    return createPortal(
        <div className="sv-settings__backdrop" onMouseDown={onClose}>
            <div
                ref={sheetRef}
                className="sv-settings"
                role="dialog"
                aria-modal="true"
                aria-label="Settings"
                tabIndex={-1}
                onMouseDown={(event) => {
                    event.stopPropagation();
                }}
            >
                <nav className="sv-settings__nav" aria-label="Settings sections">
                    <span className="sv-settings__eyebrow caps-label">Settings</span>
                    <ul className="sv-settings__list">
                        {NAV.map((item) => (
                            <li key={item.id}>
                                <button
                                    type="button"
                                    className="sv-settings__item"
                                    aria-current={item.id === section ? "page" : undefined}
                                    onClick={() => {
                                        setSection(item.id);
                                    }}
                                >
                                    <span className="sv-settings__icon">{item.icon}</span>
                                    {item.label}
                                </button>
                            </li>
                        ))}
                    </ul>
                    <span className="sv-settings__version">
                        Simple Voice
                        {info && <span className="sv-settings__build">v{info.version}</span>}
                    </span>
                </nav>
                <section className="sv-settings__pane" aria-labelledby="sv-settings-title">
                    <header className="sv-settings__header">
                        <div className="sv-settings__heading">
                            <h2 id="sv-settings-title" className="sv-settings__title">
                                {current?.label}
                            </h2>
                            <p className="sv-settings__description">{current?.description}</p>
                        </div>
                        <IconButton
                            label="Close settings"
                            icon={<X size={16} />}
                            onClick={onClose}
                        />
                    </header>
                    <div className="sv-level-line sv-settings__rule" aria-hidden="true" />
                    <div className="sv-settings__content" key={section}>
                        {section === "general" && <GeneralSection />}
                        {section === "system" && <SystemSection />}
                        {section === "models" && <ModelsSection />}
                        {section === "permissions" && <PermissionsSection />}
                        {section === "data" && <DataSection info={info} onInfoChanged={loadInfo} />}
                    </div>
                </section>
            </div>
        </div>,
        document.body,
    );
}

export function SettingsDialog({ open, onClose, initialSection }: Props) {
    if (!open) return null;
    // Mounting per opening restarts at the requested section and reloads app info.
    return <SettingsSheet key={initialSection} onClose={onClose} initialSection={initialSection} />;
}
