// FilePath: src/features/settings/settingsPages.tsx
// The settings pages in sidebar order. The sidebar lists them and SettingsPage titles itself from
// the same entry, so a label or description lives in one place.
import type { ReactNode } from "react";
import { AppWindow, AudioWaveform, Mic, ShieldCheck, WandSparkles } from "lucide-react";
import type { SettingsSection } from "../../app/ShellContext";

export interface SettingsPageInfo {
    id: SettingsSection;
    label: string;
    description: string;
    icon: ReactNode;
}

export const SETTINGS_PAGES: readonly SettingsPageInfo[] = [
    {
        id: "dictation",
        label: "Dictation",
        description: "How a recording starts, what it records and how the text arrives.",
        icon: <Mic />,
    },
    {
        id: "transcription",
        label: "Transcription",
        description: "The voice model that turns speech into text, and its languages.",
        icon: <AudioWaveform />,
    },
    {
        id: "formatting",
        label: "Formatting",
        description: "The AI that cleans up your text, and what it may learn.",
        icon: <WandSparkles />,
    },
    {
        id: "app",
        label: "App",
        description: "Appearance, startup and updates.",
        icon: <AppWindow />,
    },
    {
        id: "privacy",
        label: "Privacy & data",
        description: "What macOS allows, where your data lives, and how to clear it.",
        icon: <ShieldCheck />,
    },
];
