// FilePath: src/features/onboarding/Onboarding.tsx
// First-launch flow: welcome, microphone, accessibility, voice model, shortcuts, done. It is
// finished only when `onboardingComplete` is saved, so quitting halfway resumes here next launch.
import { useState } from "react";
import { Check } from "lucide-react";
import type { Settings } from "../../lib/api";
import { useSettings } from "../../app/SettingsContext";
import { BrandMark, Button, ShortcutKeys } from "../../ui";
import { ModelStep } from "./ModelStep";
import { PermissionStep } from "./PermissionStep";
import { ShortcutStep } from "./ShortcutStep";
import { StepFooter } from "./StepFooter";
import "../settings/common.css";
import "./Onboarding.css";

const STEPS = ["welcome", "microphone", "accessibility", "model", "shortcuts", "done"] as const;
type Step = (typeof STEPS)[number];

const STEP_LABEL: Record<Step, string> = {
    welcome: "Welcome",
    microphone: "Microphone",
    accessibility: "Accessibility",
    model: "Voice model",
    shortcuts: "Shortcuts",
    done: "Ready",
};

interface Props {
    settings: Settings;
    onDone: () => void;
}

function Welcome({ onNext }: { onNext: () => void }) {
    return (
        <>
            <BrandMark className="sv-onb__mark" />
            <h1 className="sv-onb__title sv-onb__title--hero">Speak. It&rsquo;s written.</h1>
            <p className="sv-onb__lead">
                Simple Voice turns what you say into clean, well-punctuated text and pastes it
                wherever you are typing. Hold a shortcut, talk naturally in English, Hindi or
                Hinglish, and let go. Fillers and false starts are tidied away, your own words and
                spellings are kept.
            </p>
            <StepFooter
                next={
                    <Button variant="primary" onClick={onNext}>
                        Get started
                    </Button>
                }
            />
        </>
    );
}

interface DoneProps {
    settings: Settings;
    onBack: () => void;
    onFinish: () => Promise<void>;
}

function Done({ settings, onBack, onFinish }: DoneProps) {
    const [saving, setSaving] = useState(false);
    return (
        <>
            <div className="sv-onb__icon is-success">
                <Check size={20} strokeWidth={2.2} />
            </div>
            <h1 className="sv-onb__title">You&rsquo;re all set</h1>
            <p className="sv-onb__lead">
                Simple Voice keeps running in the menu bar. Dictate into any app with your
                shortcuts; your history, insights and personal dictionary live in the main window.
            </p>
            <div className="sv-onb__summary">
                <div>
                    <span className="caps-label">Hold to speak</span>
                    <ShortcutKeys accelerator={settings.holdShortcut} />
                </div>
                <div>
                    <span className="caps-label">Toggle to speak</span>
                    <ShortcutKeys accelerator={settings.toggleShortcut} />
                </div>
            </div>
            <StepFooter
                onBack={onBack}
                next={
                    <Button
                        variant="primary"
                        loading={saving}
                        onClick={() => {
                            setSaving(true);
                            void onFinish().finally(() => {
                                setSaving(false);
                            });
                        }}
                    >
                        Start dictating
                    </Button>
                }
            />
        </>
    );
}

export function Onboarding({ settings, onDone }: Props) {
    const { update } = useSettings();
    const [step, setStep] = useState<Step>("welcome");
    const index = STEPS.indexOf(step);
    const go = (offset: number) => {
        const next = STEPS[index + offset];
        if (next) setStep(next);
    };
    const back = () => {
        go(-1);
    };
    const next = () => {
        go(1);
    };

    const finish = async () => {
        if (await update({ onboardingComplete: true })) onDone();
    };

    return (
        <div className="sv-onb">
            <div className="sv-onb__drag" data-tauri-drag-region />
            <div className="sv-onb__progress">
                <ol
                    className="sv-onb__steps"
                    aria-label={`Step ${String(index + 1)} of ${String(STEPS.length)}`}
                >
                    {STEPS.map((s, i) => (
                        <li
                            key={s}
                            className={i === index ? "is-current" : i < index ? "is-done" : ""}
                            aria-current={i === index ? "step" : undefined}
                        />
                    ))}
                </ol>
                <span className="caps-label sv-onb__step-label" aria-hidden="true">
                    <span className="sv-onb__step-count">
                        {String(index + 1).padStart(2, "0")} /{" "}
                        {String(STEPS.length).padStart(2, "0")}
                    </span>
                    {STEP_LABEL[step]}
                </span>
            </div>
            <main className="sv-onb__stage" key={step}>
                {step === "welcome" && <Welcome onNext={next} />}
                {step === "microphone" && (
                    <PermissionStep kind="microphone" onBack={back} onNext={next} />
                )}
                {step === "accessibility" && (
                    <PermissionStep kind="accessibility" onBack={back} onNext={next} />
                )}
                {step === "model" && <ModelStep onBack={back} onNext={next} />}
                {step === "shortcuts" && <ShortcutStep onBack={back} onNext={next} />}
                {step === "done" && <Done settings={settings} onBack={back} onFinish={finish} />}
            </main>
        </div>
    );
}
