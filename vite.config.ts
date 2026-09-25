// FilePath: vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// Tauri expects a fixed dev port and must see Rust compiler output, so the screen is never
// cleared and a taken port is an error instead of a silent fallback.
export default defineConfig(() => ({
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        host: host ?? false,
        hmr: host
            ? {
                  protocol: "ws",
                  host,
                  port: 1421,
              }
            : undefined,
        watch: {
            ignored: ["**/src-tauri/**", "**/engine/**", "**/target/**"],
        },
    },
}));
