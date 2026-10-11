// FilePath: mobile/src/features/setup/KeyboardSetupGroup.tsx
import { useEffect, useState } from "react";
import { Keyboard, Mic, ShieldCheck, ToggleRight } from "lucide-react";
import { useSettings } from "../../../../src/app/SettingsContext";
import { Badge, Button, SettingRow, SettingsGroup } from "../../../../src/ui";
import { keyboardReady, setupApi, type KeyboardSetup } from "./setupApi";

/** iPhone has Full Access; Android has a current keyboard instead. */
const IS_IPHONE = /iPhone|iPad|iPod/.test(navigator.userAgent);

function Status({ done }: { done: boolean }) {
    return done ? <Badge tone="success">Done</Badge> : <Badge tone="warning">To do</Badge>;
}

/**
 * Setting up the keyboard without leaving the app: each step shows whether the phone reports it
 * done, with a button that opens the right settings page. The status refreshes when the app
 * comes back from Settings (the shell remounts the tab on every return).
 */
export function KeyboardSetupGroup() {
    const { settings } = useSettings();
    const [setup, setSetup] = useState<KeyboardSetup | null>(null);
    const [busy, setBusy] = useState(false);
    // Bumped after each button, so the status is read again once the phone has acted.
    const [checks, setChecks] = useState(0);

    useEffect(() => {
        let live = true;
        setupApi
            .getKeyboardSetup()
            .then((next) => {
                if (live) setSetup(next);
            })
            .catch(() => {
                if (live) setSetup(null);
            });
        return () => {
            live = false;
        };
    }, [checks]);

    // Not on a phone, or the phone did not answer: there is nothing to show.
    if (!setup) return null;

    const run = (action: () => Promise<unknown>) => {
        setBusy(true);
        void action()
            .catch(() => undefined)
            .finally(() => {
                setBusy(false);
                setChecks((count) => count + 1);
            });
    };

    const ready = keyboardReady(setup) && settings.groqApiKeyPresent;

    return (
        <div className="svm-settings__group">
            <SettingsGroup title={ready ? "Keyboard · ready" : "Set up the keyboard"}>
                <SettingRow
                    title={
                        <span className="svm-setup__title">
                            <Keyboard aria-hidden="true" /> Add the Simple Voice keyboard
                        </span>
                    }
                    description={
                        IS_IPHONE
                            ? "Opens Simple Voice in Settings: tap Keyboards, then turn on Simple Voice."
                            : "Opens On-screen keyboard settings: turn on Simple Voice."
                    }
                >
                    {setup.enabled ? (
                        <Status done />
                    ) : (
                        <Button
                            variant="primary"
                            size="sm"
                            loading={busy}
                            onClick={() => {
                                run(setupApi.openKeyboardSettings);
                            }}
                        >
                            Open settings
                        </Button>
                    )}
                </SettingRow>
                {setup.fullAccess !== null && (
                    <SettingRow
                        title={
                            <span className="svm-setup__title">
                                <ShieldCheck aria-hidden="true" /> Allow Full Access
                            </span>
                        }
                        description="The keyboard needs it to send your voice to the app. It turns on in the same Keyboards page."
                    >
                        {setup.fullAccess ? (
                            <Status done />
                        ) : (
                            <Button
                                size="sm"
                                loading={busy}
                                onClick={() => {
                                    run(setupApi.openKeyboardSettings);
                                }}
                            >
                                Open settings
                            </Button>
                        )}
                    </SettingRow>
                )}
                {setup.selected !== null && (
                    <SettingRow
                        title={
                            <span className="svm-setup__title">
                                <ToggleRight aria-hidden="true" /> Switch to Simple Voice
                            </span>
                        }
                        description="Makes it the keyboard that opens when you tap a text field."
                    >
                        {setup.selected ? (
                            <Status done />
                        ) : (
                            <Button
                                size="sm"
                                loading={busy}
                                disabled={!setup.enabled}
                                onClick={() => {
                                    run(setupApi.chooseKeyboard);
                                }}
                            >
                                Choose keyboard
                            </Button>
                        )}
                    </SettingRow>
                )}
                <SettingRow
                    title={
                        <span className="svm-setup__title">
                            <Mic aria-hidden="true" /> Allow the microphone
                        </span>
                    }
                    description={
                        setup.microphone === "denied"
                            ? "It was turned off; the button opens Settings to turn it back on."
                            : "The app records what you say in the keyboard."
                    }
                >
                    {setup.microphone === "granted" ? (
                        <Status done />
                    ) : (
                        <Button
                            size="sm"
                            loading={busy}
                            onClick={() => {
                                run(setupApi.requestMicrophone);
                            }}
                        >
                            Allow
                        </Button>
                    )}
                </SettingRow>
                {!settings.groqApiKeyPresent && (
                    <SettingRow
                        title="Add your Groq key"
                        description="Dictation sends your voice to Groq. Add the key in Voice below."
                    >
                        <Button
                            size="sm"
                            onClick={() => {
                                document
                                    .getElementById("svm-voice")
                                    ?.scrollIntoView({ behavior: "smooth" });
                            }}
                        >
                            Go to key
                        </Button>
                    </SettingRow>
                )}
            </SettingsGroup>
        </div>
    );
}
