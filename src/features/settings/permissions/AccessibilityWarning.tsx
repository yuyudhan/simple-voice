// FilePath: src/features/settings/permissions/AccessibilityWarning.tsx
// Accessibility access is what lets Simple Voice press ⌘V in the app being dictated into, and the
// only way macOS feeds the event tap that watches the Fn key. Without it dictated text is only
// copied and an Fn shortcut does nothing, so this warning cannot be dismissed: it stays up until
// access is granted. The shell pins it above every page; next to the shortcut settings it only
// appears while Fn is one of the shortcuts, because that is what it explains there.
import { TriangleAlert } from "lucide-react";
import { useSettings } from "../../../app/SettingsContext";
import { FN_KEY } from "../shortcuts/accelerator";
import { PermissionAction } from "./PermissionsSection";
import { usePermissions } from "./usePermissions";
import "./AccessibilityWarning.css";

export function AccessibilityWarning({ placement }: { placement: "shell" | "inline" }) {
    const { settings } = useSettings();
    const { permissions, busy, request, openSettings } = usePermissions();
    const fnModes = [
        settings.holdShortcut === FN_KEY ? "hold to speak" : null,
        settings.toggleShortcut === FN_KEY ? "toggle to speak" : null,
    ].filter((mode) => mode !== null);
    const status = permissions?.accessibility;
    if (status === undefined || status === "granted") return null;
    if (placement === "inline" && fnModes.length === 0) return null;

    return (
        <div className={`sv-access-warning sv-access-warning--${placement}`} role="alert">
            <TriangleAlert size={16} className="sv-access-warning__icon" aria-hidden="true" />
            {fnModes.length === 0 ? (
                <p className="sv-access-warning__text">
                    <strong>Text is only copied, not pasted.</strong> Simple Voice needs
                    Accessibility access to press ⌘V in the app you are using.
                </p>
            ) : (
                <p className="sv-access-warning__text">
                    <strong>Pasting and the fn key do not work yet.</strong> Simple Voice needs
                    Accessibility access to press ⌘V for you and to use fn for{" "}
                    {fnModes.join(" and ")}.
                </p>
            )}
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
