// FilePath: src/features/onboarding/PermissionStep.tsx
// Microphone and Accessibility steps. Status is polled because macOS does not tell the app when
// the user flips the switch in System Settings.
import type { ReactNode } from "react";
import { CircleCheck, Keyboard, Mic } from "lucide-react";
import { Button, Spinner } from "../../ui";
import { StatusBadge } from "../settings/permissions/PermissionsSection";
import { usePermissions } from "../settings/permissions/usePermissions";
import { StepFooter } from "./StepFooter";

const POLL_MS = 1500;

interface Copy {
    icon: ReactNode;
    label: string;
    title: string;
    body: string;
    grant: string;
    deniedHelp: string;
}

const COPY: Record<"microphone" | "accessibility", Copy> = {
    microphone: {
        icon: <Mic size={20} />,
        label: "Microphone access",
        title: "Let Simple Voice hear you",
        body: "Microphone access is needed to dictate. Audio is recorded only while you hold or toggle your shortcut.",
        grant: "Allow microphone",
        deniedHelp:
            "Microphone access was turned off. Open System Settings → Privacy & Security → Microphone and switch Simple Voice on.",
    },
    accessibility: {
        icon: <Keyboard size={20} />,
        label: "Accessibility access",
        title: "Paste into any app",
        body: "Accessibility access lets Simple Voice press ⌘V for you, so your words land in whatever text field you are typing in — Mail, Slack, your editor, anywhere.",
        grant: "Grant access",
        deniedHelp:
            "In System Settings → Privacy & Security → Accessibility, switch Simple Voice on. This page updates by itself once it is enabled.",
    },
};

interface Props {
    kind: "microphone" | "accessibility";
    onBack: () => void;
    onNext: () => void;
}

export function PermissionStep({ kind, onBack, onNext }: Props) {
    const { permissions, error, busy, request, openSettings } = usePermissions(POLL_MS);
    const copy = COPY[kind];
    const status = permissions?.[kind];
    const granted = status === "granted";

    let action: ReactNode = null;
    if (granted) {
        action = (
            <p className="sv-onb__granted">
                <CircleCheck size={16} /> All set
            </p>
        );
    } else if (status === "not_determined" || (kind === "accessibility" && status === "denied")) {
        action = (
            <div className="sv-onb__actions">
                <Button
                    variant="primary"
                    loading={busy === kind}
                    onClick={() => {
                        void request(kind);
                    }}
                >
                    {copy.grant}
                </Button>
                {kind === "accessibility" && (
                    <Button
                        variant="secondary"
                        onClick={() => {
                            void openSettings(kind);
                        }}
                    >
                        Open System Settings
                    </Button>
                )}
            </div>
        );
    } else if (status !== undefined) {
        action = (
            <div className="sv-onb__actions">
                <Button
                    variant="primary"
                    onClick={() => {
                        void openSettings(kind);
                    }}
                >
                    Open System Settings
                </Button>
            </div>
        );
    }

    const showDeniedHelp =
        !granted && (status === "denied" || status === "restricted") && kind === "microphone";

    return (
        <>
            <div className="sv-onb__icon">{copy.icon}</div>
            <h1 className="sv-onb__title">{copy.title}</h1>
            <p className="sv-onb__lead">{copy.body}</p>
            <section className="sv-onb__panel" aria-label={copy.label}>
                <div className="sv-onb__panel-row">
                    <div className="sv-onb__panel-status">
                        <span className="caps-label">{copy.label}</span>
                        {status === undefined ? <Spinner /> : <StatusBadge status={status} />}
                    </div>
                    {action}
                </div>
                {(showDeniedHelp || (kind === "accessibility" && !granted) || error) && (
                    <div className="sv-onb__panel-foot">
                        {!granted && (showDeniedHelp || kind === "accessibility") && (
                            <p className="sv-inline-note">{copy.deniedHelp}</p>
                        )}
                        {error && (
                            <p className="sv-inline-error" role="alert">
                                {error}
                            </p>
                        )}
                    </div>
                )}
            </section>
            <StepFooter
                onBack={onBack}
                next={
                    <Button variant="primary" disabled={!granted} onClick={onNext}>
                        Continue
                    </Button>
                }
                skip={
                    kind === "accessibility" && !granted
                        ? {
                              label: "Skip for now",
                              warning:
                                  "Without Accessibility, dictated text is only copied to the clipboard and you paste it yourself.",
                              onSkip: onNext,
                          }
                        : undefined
                }
            />
        </>
    );
}
