// FilePath: src/app/ShellContext.tsx
import { createContext, useContext } from "react";

export type SettingsSection = "general" | "system" | "models" | "permissions" | "data";

export type Page = "home" | "insights" | "dictionary" | "style";

export interface ShellContextValue {
    openSettings: (section?: SettingsSection) => void;
    navigate: (page: Page) => void;
}

export const ShellContext = createContext<ShellContextValue | null>(null);

/** Lets pages open the Settings dialog at a section or switch pages without prop drilling. */
export function useShell(): ShellContextValue {
    const context = useContext(ShellContext);
    if (!context) throw new Error("useShell must be used inside the app shell");
    return context;
}
