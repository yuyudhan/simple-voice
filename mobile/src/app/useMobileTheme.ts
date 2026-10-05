// FilePath: mobile/src/app/useMobileTheme.ts
import { useEffect, useState } from "react";
import type { Theme } from "../../../src/lib/api";

// Same cache key as the desktop theme module, so its paintCachedTheme() paints the first frame
// in the chosen theme. Phones have no window chrome to recolour, so only the page follows.
const CACHE_KEY = "sv.theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

/** Applies the theme to the page and follows the system appearance while it is "system". */
export function useMobileTheme(theme: Theme) {
    const [systemDark, setSystemDark] = useState(darkQuery.matches);
    const resolved = theme === "system" ? (systemDark ? "dark" : "light") : theme;

    useEffect(() => {
        const follow = () => {
            setSystemDark(darkQuery.matches);
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
    }, [theme]);
}
