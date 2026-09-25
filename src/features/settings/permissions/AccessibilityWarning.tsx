// FilePath: src/features/settings/shortcuts/FnKeyAccessWarning.tsx
// The Fn key is watched through an event tap that macOS only feeds once Simple Voice has
// Accessibility access. Without it the Fn shortcut does nothing at all, so this warning cannot be
// dismissed: it stays up for as long as Fn is configured and access is missing.
import { TriangleAlert } from "lucide-react";
import { useSettings } from "../../../app/SettingsContext";
import { PermissionAction } from "../permissions/PermissionsSection";
import { usePermissions } from "../permissions/usePermissions";
import { FN_KEY } from "./accelerator";
import "./FnKeyAccessWarning.css";

export function FnKeyAccessWarning({ placement }: { placement: "shell" | "inline" }) {
    const { settings } = useSettings();
    const { permissions, busy, request, openSettings } = usePermissions();
    const modes = [
        settings.holdShortcut === FN_KEY ? "hold to speak" : null,
        settings.toggleShortcut === FN_KEY ? "toggle to speak" : null,
    ].filter((mode) => mode !== null);
    const status = permissions?.accessibility;
    if (modes.length === 0 || status === undefined || status === "granted") return null;

    return (
        <div className={`sv-fn-warning sv-fn-warning--${placement}`} role="alert">
            <TriangleAlert size={16} className="sv-fn-warning__icon" aria-hidden="true" />
            <p className="sv-fn-warning__text">
                <strong>The fn key does not work yet.</strong> Simple Voice needs Accessibility
                access to use fn for {modes.join(" and ")}.
            </p>
            <PermissionAction
                kind="accessibility"
                status={status}
                busy={busy === "accessibility"}
                onRequest={request}
                onOpenSettings={openSettings}
            />
        </div>
    );
}
