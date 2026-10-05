// FilePath: mobile/src/main.tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { PlatformContext } from "../../src/app/PlatformContext";
import { SettingsProvider } from "../../src/app/SettingsContext";
import { paintCachedTheme } from "../../src/app/theme";
import { ToastProvider } from "../../src/ui";
import { MobileApp } from "./app/MobileApp";
import "../../src/styles/global.css";
import "./app/mobile.css";

const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing #root");

paintCachedTheme();

createRoot(root).render(
    <StrictMode>
        <PlatformContext.Provider value="mobile">
            <ToastProvider>
                <SettingsProvider>
                    <MobileApp />
                </SettingsProvider>
            </ToastProvider>
        </PlatformContext.Provider>
    </StrictMode>,
);
