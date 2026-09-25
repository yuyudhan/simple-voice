// FilePath: src/app/theme.ts
import { useEffect } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { errorMessage, type Theme } from "../lib/api";

type Resolved = "light" | "dark";

// The database is the source of truth, but it is only readable after the backend answers; this
// cache lets the first frame paint in the user's theme instead of flashing the other palette.
const CACHE_KEY = "sv.theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function isTheme(value: string | null): value is Theme {
    return value === "system" || value === "light" || value === "dark";
}

function resolve(theme: Theme): Resolved {
    if (theme !== "system") return theme;
    return darkQuery.matches ? "dark" : "light";
}

function paint(theme: Theme) {
    document.documentElement.dataset.theme = resolve(theme);
}

/** Paints the last chosen theme synchronously, before React mounts. */
export function paintCachedTheme() {
    const cached = window.localStorage.getItem(CACHE_KEY);
    paint(isTheme(cached) ? cached : "system");
}

/**
 * Applies the theme to the page and the native window chrome (title bar, traffic lights), and
 * follows the OS appearance live while the theme is "system".
 */
export function useTheme(theme: Theme, onError: (message: string) => void) {
    useEffect(() => {
        window.localStorage.setItem(CACHE_KEY, theme);
        paint(theme);
        getCurrentWindow()
            .setTheme(theme === "system" ? null : theme)
            .catch((error: unknown) => {
                onError(errorMessage(error));
            });
        if (theme !== "system") return undefined;
        const follow = () => {
            paint(theme);
        };
        darkQuery.addEventListener("change", follow);
        return () => {
            darkQuery.removeEventListener("change", follow);
        };
    }, [theme, onError]);
}
