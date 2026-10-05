// FilePath: mobile/vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import process from "node:process";

const host = process.env.TAURI_DEV_HOST;

// The mobile screens reuse the desktop React code in ../src, so the dev server may read the
// parent folder. Port 1430 keeps it apart from the desktop dev server (1420); a phone reaches
// it over the network through the host Tauri passes in TAURI_DEV_HOST.
export default defineConfig(() => ({
    root: import.meta.dirname,
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 1430,
        strictPort: true,
        host: host ?? false,
        hmr: host
            ? {
                  protocol: "ws",
                  host,
                  port: 1431,
              }
            : undefined,
        fs: {
            allow: [".."],
        },
        watch: {
            ignored: ["**/src-tauri/**", "**/target/**"],
        },
    },
    build: {
        outDir: "dist",
        emptyOutDir: true,
    },
}));
