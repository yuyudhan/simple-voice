// FilePath: src/app/ShellContext.tsx
import { createContext, useContext } from "react";

const SETTINGS_SECTIONS = ["general", "system", "models", "permissions", "data"] as const;

export type SettingsSection = (typeof SETTINGS_SECTIONS)[number];

/** Every page of the main window; each settings section is a page of its own. */
export type Page = "home" | "insights" | "dictionary" | "style" | SettingsSection;

export function isSettingsSection(page: Page): page is SettingsSection {
    return SETTINGS_SECTIONS.some((section) => section === page);
}

export interface ShellContextValue {
    navigate: (page: Page) => void;
}

export const ShellContext = createContext<ShellContextValue | null>(null);

/** Lets pages switch to another page, a settings section included, without prop drilling. */
export function useShell(): ShellContextValue {
    const context = useContext(ShellContext);
    if (!context) throw new Error("useShell must be used inside the app shell");
    return context;
}
