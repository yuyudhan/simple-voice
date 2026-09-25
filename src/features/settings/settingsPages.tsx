// FilePath: src/features/settings/settingsPages.tsx
// The settings pages in sidebar order. The sidebar lists them and SettingsPage titles itself from
// the same entry, so a label or description lives in one place.
import type { ReactNode } from "react";
import { Cpu, Database, Monitor, ShieldCheck, SlidersHorizontal } from "lucide-react";
import type { SettingsSection } from "../../app/ShellContext";

export interface SettingsPageInfo {
    id: SettingsSection;
    label: string;
    description: string;
    icon: ReactNode;
}

export const SETTINGS_PAGES: readonly SettingsPageInfo[] = [
    {
        id: "general",
        label: "General",
        description: "Appearance, shortcuts, microphone and dictation languages.",
        icon: <SlidersHorizontal />,
    },
    {
        id: "system",
        label: "System",
        description: "Startup, sound cues and how dictated text is delivered.",
        icon: <Monitor />,
    },
    {
        id: "models",
        label: "Models",
        description: "The voice model that transcribes and the provider that formats.",
        icon: <Cpu />,
    },
    {
        id: "permissions",
        label: "Permissions",
        description: "What macOS allows Simple Voice to do.",
        icon: <ShieldCheck />,
    },
    {
        id: "data",
        label: "Data",
        description: "Where your history is stored, and how to clear it.",
        icon: <Database />,
    },
];
