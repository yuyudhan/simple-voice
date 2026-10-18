// FilePath: mobile/src/features/setup/KeyboardSetupGroup.tsx
import { useSettings } from "../../../../src/app/SettingsContext";
import { Badge, Button, SettingRow, SettingsGroup } from "../../../../src/ui";
import type { KeyboardRequest, KeyboardStep } from "./setupApi";
import { setupSteps } from "./setupSteps";
import { useKeyboardSetup } from "./useKeyboardSetup";

function scrollToKey() {
    document.getElementById("svm-voice")?.scrollIntoView({ behavior: "smooth" });
}

/** The checklist is the microphone step itself; only the key lives elsewhere on the page. */
function showStep(step: KeyboardStep) {
    if (step === "groqKey") scrollToKey();
}

/**
 * Setting up the keyboard without leaving the app: each step shows whether the phone reports it
 * done, with a button that opens the right settings page (the Groq key scrolls to its field in
 * Voice). The last row opens the first-launch guide again.
 */
export function KeyboardSetupGroup({
    request,
    onRequest,
    onOpenGuide,
}: {
    request: KeyboardRequest | null;
    onRequest: (next: KeyboardRequest | null) => void;
    onOpenGuide: () => void;
}) {
    const { settings } = useSettings();
    const { setup, busy, run } = useKeyboardSetup(request, onRequest, showStep);

    // Not on a phone, or the phone did not answer: there is nothing to show.
    if (!setup) return null;

    const steps = setupSteps(setup, settings.groqApiKeyPresent);
    const next = steps.find((step) => !step.done);

    return (
        <div className="svm-settings__group">
            <SettingsGroup title={next ? "Set up the keyboard" : "Keyboard · ready"}>
                {steps.map(({ id, icon: Icon, title, description, done, action }) => (
                    <SettingRow
                        key={id}
                        title={
                            <span className="svm-setup__title">
                                <Icon aria-hidden="true" /> {title}
                            </span>
                        }
                        description={description}
                    >
                        {done ? (
                            <Badge tone="success">Done</Badge>
                        ) : (
                            <Button
                                variant={id === next?.id ? "primary" : "secondary"}
                                size="sm"
                                loading={action !== null && busy}
                                disabled={action?.disabled}
                                onClick={() => {
                                    if (action) run(action.run);
                                    else scrollToKey();
                                }}
                            >
                                {action ? action.label : "Go to key"}
                            </Button>
                        )}
                    </SettingRow>
                ))}
                <SettingRow
                    title="Setup guide"
                    description="The same steps one at a time, as on first launch."
                >
                    <Button size="sm" onClick={onOpenGuide}>
                        Open guide
                    </Button>
                </SettingRow>
            </SettingsGroup>
        </div>
    );
}
