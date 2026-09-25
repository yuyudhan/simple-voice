// FilePath: src/main.tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./app/App";
import { SettingsProvider } from "./app/SettingsContext";
import { paintCachedTheme } from "./app/theme";
import { Overlay } from "./features/overlay/Overlay";
import { ToastProvider } from "./ui";
import "./styles/global.css";

const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing #root");

// Both windows load this bundle; the label decides which surface to mount. The overlay pill is
// self-contained: it must paint on the first frame and never show toasts or a loading screen.
const isOverlay = getCurrentWindow().label === "overlay";
if (isOverlay) document.documentElement.classList.add("overlay-window");
else paintCachedTheme();

createRoot(root).render(
    <StrictMode>
        {isOverlay ? (
            <Overlay />
        ) : (
            <ToastProvider>
                <SettingsProvider>
                    <App />
                </SettingsProvider>
            </ToastProvider>
        )}
    </StrictMode>,
);
