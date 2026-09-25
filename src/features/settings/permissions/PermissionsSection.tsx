// FilePath: src/features/settings/permissions/PermissionsSection.tsx
import { CircleCheck } from "lucide-react";
import type { PermissionKind, PermissionStatus } from "../../../lib/api";
import { Badge, Button, SettingRow, SettingsGroup, Spinner } from "../../../ui";
import { usePermissions } from "./usePermissions";
import "../common.css";
import "./permissions.css";

interface PermissionCopy {
    kind: PermissionKind;
    title: string;
    requirement: string;
    why: string;
}

const PERMISSIONS: PermissionCopy[] = [
    {
        kind: "microphone",
        title: "Microphone",
        requirement: "Required",
        why: "Needed to hear you. Audio is recorded only while you hold or toggle the shortcut.",
    },
    {
        kind: "accessibility",
        title: "Accessibility",
        requirement: "Required to paste",
        why: "Lets Simple Voice press ⌘V in the app you are using. Without it text is only copied.",
    },
    {
        kind: "speech",
        title: "Speech Recognition",
        requirement: "Apple Speech only",
        why: "Only needed when Apple Speech is your voice model; other models never use it.",
    },
];

export function StatusBadge({ status }: { status: PermissionStatus }) {
    switch (status) {
        case "granted":
            return <Badge tone="success">Granted</Badge>;
        case "denied":
            return <Badge tone="danger">Denied</Badge>;
        case "restricted":
            return <Badge tone="warning">Restricted</Badge>;
        case "not_determined":
            return <Badge tone="neutral">Not requested</Badge>;
    }
}

interface ActionProps {
    kind: PermissionKind;
    status: PermissionStatus;
    busy: boolean;
    onRequest: (kind: PermissionKind) => Promise<void>;
    onOpenSettings: (kind: PermissionKind) => Promise<void>;
}

/** Grant when macOS can still show its prompt, otherwise send the user to System Settings. */
export function PermissionAction({ kind, status, busy, onRequest, onOpenSettings }: ActionProps) {
    if (status === "granted") {
        return <CircleCheck size={20} className="sv-perm__check" aria-label="Granted" />;
    }
    if (status === "not_determined") {
        return (
            <Button
                size="sm"
                variant="primary"
                loading={busy}
                onClick={() => {
                    void onRequest(kind);
                }}
            >
                Grant
            </Button>
        );
    }
    return (
        <Button
            size="sm"
            variant="secondary"
            onClick={() => {
                void onOpenSettings(kind);
            }}
        >
            Open System Settings
        </Button>
    );
}

export function PermissionsSection() {
    const { permissions, error, busy, request, openSettings } = usePermissions();

    return (
        <SettingsGroup title="macOS permissions">
            {permissions === null ? (
                <div className="sv-perm__loading">
                    {error ? (
                        <p className="sv-inline-error" role="alert">
                            {error}
                        </p>
                    ) : (
                        <Spinner />
                    )}
                </div>
            ) : (
                PERMISSIONS.map((p) => (
                    <SettingRow
                        key={p.kind}
                        title={p.title}
                        description={`${p.requirement}. ${p.why}`}
                    >
                        <div className="sv-perm__control">
                            <StatusBadge status={permissions[p.kind]} />
                            {permissions[p.kind] !== "granted" && (
                                <PermissionAction
                                    kind={p.kind}
                                    status={permissions[p.kind]}
                                    busy={busy === p.kind}
                                    onRequest={request}
                                    onOpenSettings={openSettings}
                                />
                            )}
                        </div>
                    </SettingRow>
                ))
            )}
            {permissions !== null && error && (
                <p className="sv-inline-error sv-perm__error" role="alert">
                    {error}
                </p>
            )}
        </SettingsGroup>
    );
}
