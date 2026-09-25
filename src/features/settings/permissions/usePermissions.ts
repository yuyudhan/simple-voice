// FilePath: src/features/settings/permissions/usePermissions.ts
// Live permission status. macOS does not notify apps when the user flips a switch in System
// Settings, so besides the backend event this re-reads on window focus and, when asked, polls.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, events, type PermissionKind, type Permissions } from "../../../lib/api";
import { useTauriEvent } from "../../../lib/useTauriEvent";

export function usePermissions(pollMs?: number) {
    const [permissions, setPermissions] = useState<Permissions | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState<PermissionKind | null>(null);

    const refresh = useCallback(
        () =>
            api.getPermissions().then(
                (next) => {
                    setPermissions(next);
                    setError(null);
                },
                (e: unknown) => {
                    setError(errorMessage(e));
                },
            ),
        [],
    );

    useEffect(() => {
        void refresh();
        const onFocus = () => {
            void refresh();
        };
        window.addEventListener("focus", onFocus);
        return () => {
            window.removeEventListener("focus", onFocus);
        };
    }, [refresh]);

    useEffect(() => {
        if (pollMs === undefined) return;
        const timer = window.setInterval(() => {
            void refresh();
        }, pollMs);
        return () => {
            window.clearInterval(timer);
        };
    }, [pollMs, refresh]);

    useTauriEvent(events.permissionsChanged, setPermissions);

    const request = useCallback(async (kind: PermissionKind) => {
        setBusy(kind);
        try {
            setPermissions(await api.requestPermission(kind));
            setError(null);
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setBusy(null);
        }
    }, []);

    const openSettings = useCallback(async (kind: PermissionKind) => {
        try {
            await api.openPermissionSettings(kind);
        } catch (e) {
            setError(errorMessage(e));
        }
    }, []);

    return { permissions, error, busy, refresh, request, openSettings };
}
