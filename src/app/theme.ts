// FilePath: src/app/theme.ts
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { errorMessage, type Theme } from "../lib/api";

export type ResolvedTheme = "light" | "dark";

// The database is the source of truth, but it is only readable after the backend answers; this
// cache lets the first frame paint in the user's theme instead of flashing the other palette.
const CACHE_KEY = "sv.theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function isTheme(value: string | null): value is Theme {
    return value === "system" || value === "light" || value === "dark";
}

function resolve(theme: Theme, osDark: boolean): ResolvedTheme {
    if (theme !== "system") return theme;
    return osDark ? "dark" : "light";
}

/** Paints the last chosen theme synchronously, before React mounts. */
export function paintCachedTheme() {
    const cached = window.localStorage.getItem(CACHE_KEY);
    document.documentElement.dataset.theme = resolve(
        isTheme(cached) ? cached : "system",
        darkQuery.matches,
    );
}

/**
 * Applies the theme to the page and the native window chrome (title bar, traffic lights),
 * follows the OS appearance live while the theme is "system", and returns what is showing.
 */
export function useTheme(theme: Theme, onError: (message: string) => void): ResolvedTheme {
    const [osDark, setOsDark] = useState(darkQuery.matches);
    const resolved = resolve(theme, osDark);

    // The webview reports the native window appearance, so this also fires after setTheme(null)
    // hands the window back to macOS.
    useEffect(() => {
        const follow = () => {
            setOsDark(darkQuery.matches);
        };
        darkQuery.addEventListener("change", follow);
        return () => {
            darkQuery.removeEventListener("change", follow);
        };
    }, []);

    useEffect(() => {
        document.documentElement.dataset.theme = resolved;
    }, [resolved]);

    useEffect(() => {
        window.localStorage.setItem(CACHE_KEY, theme);
        getCurrentWindow()
            .setTheme(theme === "system" ? null : theme)
            .catch((error: unknown) => {
                onError(errorMessage(error));
            });
    }, [theme, onError]);

    return resolved;
}
