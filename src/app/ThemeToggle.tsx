// FilePath: src/app/ThemeToggle.tsx
import { Moon, Sun } from "lucide-react";
import { IconButton } from "../ui";
import { useSettings } from "./SettingsContext";
import type { ResolvedTheme } from "./theme";
import "./ThemeToggle.css";

/**
 * One-click switch between light and dark in the window's top-right corner. It flips what is
 * showing, so from "system" it pins the opposite of the current macOS appearance; Settings →
 * General → Appearance returns to "system".
 */
export function ThemeToggle({ resolved }: { resolved: ResolvedTheme }) {
    const { update } = useSettings();
    const next = resolved === "dark" ? "light" : "dark";
    return (
        <IconButton
            className="sv-theme-toggle"
            label={next === "dark" ? "Switch to dark mode" : "Switch to light mode"}
            icon={next === "dark" ? <Moon /> : <Sun />}
            onClick={() => {
                void update({ theme: next });
            }}
        />
    );
}
