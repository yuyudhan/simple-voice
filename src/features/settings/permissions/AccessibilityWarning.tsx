// FilePath: src/features/settings/permissions/AccessibilityWarning.tsx
// Accessibility access is what lets Simple Voice press ⌘V in the app being dictated into, and the
// only way macOS feeds the event tap that watches the Fn key. Without it dictated text is only
// copied and an Fn shortcut does nothing, so this warning cannot be dismissed: it stays up until
// access is granted. App-wide, the health banner reports missing access (and, on Linux, a
// missing paste tool); this one sits next to the shortcut settings and only appears while Fn,
// a macOS-only key, is one of the shortcuts, because that is what it explains.
import { TriangleAlert } from "lucide-react";
import { useSettings } from "../../../app/SettingsContext";
import { FN_KEY } from "../shortcuts/accelerator";
import { PermissionAction } from "./PermissionsGroup";
import { usePermissions } from "./usePermissions";
import "./AccessibilityWarning.css";

export function AccessibilityWarning() {
    const { settings } = useSettings();
    const { permissions, busy, request, openSettings } = usePermissions();
    const fnModes = [
        settings.holdShortcut === FN_KEY ? "hold to speak" : null,
        settings.toggleShortcut === FN_KEY ? "toggle to speak" : null,
    ].filter((mode) => mode !== null);
    const status = permissions?.accessibility;
    if (status === undefined || status === "granted" || fnModes.length === 0) return null;

    return (
        <div className="sv-access-warning" role="alert">
            <TriangleAlert size={16} className="sv-access-warning__icon" aria-hidden="true" />
            <p className="sv-access-warning__text">
                <strong>Pasting and the fn key do not work yet.</strong> Simple Voice needs
                Accessibility access to press ⌘V for you and to use fn for {fnModes.join(" and ")}.
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
