// FilePath: src/main.tsx
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { SettingsProvider } from "./app/SettingsContext";
import { paintCachedTheme } from "./app/theme";
import { ToastProvider } from "./ui";
import "./styles/global.css";

const root = document.getElementById("root");
if (!root) throw new Error("index.html is missing #root");

paintCachedTheme();

createRoot(root).render(
    <StrictMode>
        <ToastProvider>
            <SettingsProvider>
                <App />
            </SettingsProvider>
        </ToastProvider>
    </StrictMode>,
);
