// FilePath: mobile/src/features/setup/SetupGuide.tsx
// First-launch guide: a welcome, then the setup steps one at a time (Groq key, keyboard, Full
// Access or current keyboard, microphone), each with its live state and a skip. Finishing or
// skipping saves `onboardingComplete`, so it shows once; Settings opens it again. The look is
// the desktop onboarding's (Signal), with its footer.
import { useCallback, useState } from "react";
import { useSettings } from "../../../../src/app/SettingsContext";
import { StepFooter } from "../../../../src/features/onboarding/StepFooter";
import { GroqKeyField } from "../../../../src/features/settings/models/GroqKeyField";
import { Badge, BrandMark, Button } from "../../../../src/ui";
import "../../../../src/features/onboarding/Onboarding.css";
import type { KeyboardRequest, KeyboardStep } from "./setupApi";
import { setupSteps, type SetupStep, type SetupStepId } from "./setupSteps";
import { useKeyboardSetup } from "./useKeyboardSetup";

type Screen = "welcome" | SetupStepId;

const LABEL: Record<Screen, string> = {
    welcome: "Welcome",
    groqKey: "Groq key",
    keyboard: "Keyboard",
    fullAccess: "Full Access",
    selected: "Switch",
    microphone: "Microphone",
};

const SKIP_WARNING: Record<SetupStepId, string> = {
    groqKey: "Without a Groq key the keyboard types, but it can't dictate.",
    keyboard: "Without the keyboard there is nowhere to dictate.",
    fullAccess: "Without Full Access the keyboard can't reach the app to record you.",
    selected: "Until you switch, text fields open your current keyboard.",
    microphone: 'Until it is allowed, the keyboard shows "Microphone off" and its mic stays off.',
};

interface StepProps {
    step: SetupStep;
    busy: boolean;
    run: (action: () => Promise<unknown>) => void;
}

function StepBody({ step, busy, run }: StepProps) {
    const { icon: Icon, action } = step;
    return (
        <>
            <div className={step.done ? "sv-onb__icon is-success" : "sv-onb__icon"}>
                <Icon size={20} aria-hidden="true" />
            </div>
            <h1 className="sv-onb__title">{step.title}</h1>
            <p className="sv-onb__lead">{step.description}</p>
            <section className="sv-onb__panel" aria-label={step.title}>
                <div className="sv-onb__panel-row">
                    <div className="sv-onb__panel-status">
                        <span className="caps-label">Status</span>
                        {step.done ? (
                            <Badge tone="success">Done</Badge>
                        ) : (
                            <Badge tone="warning">To do</Badge>
                        )}
                    </div>
                    {!step.done && action && (
                        <Button
                            variant="primary"
                            loading={busy}
                            disabled={action.disabled}
                            onClick={() => {
                                run(action.run);
                            }}
                        >
                            {action.label}
                        </Button>
                    )}
                </div>
                {step.id === "groqKey" && (
                    <div className="sv-onb__panel-foot">
                        <GroqKeyField />
                    </div>
                )}
                {!step.done && action?.disabled && (
                    <div className="sv-onb__panel-foot">
                        <p className="sv-inline-note">Add the keyboard first.</p>
                    </div>
                )}
            </section>
        </>
    );
}

export function SetupGuide({
    request,
    onRequest,
    onClose,
}: {
    request: KeyboardRequest | null;
    onRequest: (next: KeyboardRequest | null) => void;
    onClose: () => void;
}) {
    const { settings, update } = useSettings();
    const [screen, setScreen] = useState<Screen>("welcome");
    const [saving, setSaving] = useState(false);
    // A keyboard request arriving now jumps to its step; the hook runs the request itself.
    const showStep = useCallback((step: KeyboardStep) => {
        setScreen(step);
    }, []);
    const { setup, busy, run } = useKeyboardSetup(request, onRequest, showStep);

    const steps = setupSteps(setup, settings.groqApiKeyPresent);
    const screens: Screen[] = ["welcome", ...steps.map((step) => step.id)];
    // A step can drop out of the list while the phone's answer is pending; start over then.
    const index = Math.max(0, screens.indexOf(screen));
    const step = steps[index - 1];
    const last = index === screens.length - 1;

    const finish = () => {
        setSaving(true);
        void update({ onboardingComplete: true }).then((saved) => {
            if (saved) onClose();
            else setSaving(false);
        });
    };
    const go = (offset: number) => {
        const target = screens[index + offset];
        if (target) setScreen(target);
    };
    const next = () => {
        if (last) finish();
        else go(1);
    };

    return (
        <div className="sv-onb svm-guide">
            <div className="sv-onb__progress">
                <ol
                    className="sv-onb__steps"
                    aria-label={`Step ${String(index + 1)} of ${String(screens.length)}`}
                >
                    {screens.map((id, i) => (
                        <li
                            key={id}
                            className={i === index ? "is-current" : i < index ? "is-done" : ""}
                            aria-current={i === index ? "step" : undefined}
                        />
                    ))}
                </ol>
                <span className="caps-label sv-onb__step-label" aria-hidden="true">
                    <span className="sv-onb__step-count">
                        {String(index + 1).padStart(2, "0")} /{" "}
                        {String(screens.length).padStart(2, "0")}
                    </span>
                    {LABEL[screens[index] ?? "welcome"]}
                </span>
            </div>
            <main className="sv-onb__stage" key={screens[index]}>
                {step ? (
                    <>
                        <StepBody step={step} busy={busy} run={run} />
                        <StepFooter
                            onBack={() => {
                                go(-1);
                            }}
                            next={
                                <Button
                                    variant="primary"
                                    disabled={!step.done}
                                    loading={saving}
                                    onClick={next}
                                >
                                    {last ? "Finish" : "Next"}
                                </Button>
                            }
                            skip={
                                step.done
                                    ? undefined
                                    : {
                                          label: "Skip",
                                          warning: SKIP_WARNING[step.id],
                                          onSkip: next,
                                      }
                            }
                        />
                    </>
                ) : (
                    <>
                        <BrandMark className="sv-onb__mark" />
                        <h1 className="sv-onb__title sv-onb__title--hero">
                            Welcome to Simple Voice
                        </h1>
                        <p className="sv-onb__lead">
                            Dictate in any app with the Simple Voice keyboard: tap its mic, speak,
                            and clean text lands where you are typing. A few steps get it ready;
                            skip any of them and finish later in Settings.
                        </p>
                        <StepFooter
                            next={
                                <Button variant="primary" onClick={next}>
                                    Set up
                                </Button>
                            }
                            skip={{
                                label: "Skip setup",
                                warning:
                                    "Every step stays in Settings, which opens this guide again.",
                                onSkip: finish,
                            }}
                        />
                    </>
                )}
            </main>
        </div>
    );
}
