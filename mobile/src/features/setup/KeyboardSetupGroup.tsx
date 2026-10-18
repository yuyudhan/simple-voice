// FilePath: mobile/src/features/setup/KeyboardSetupGroup.tsx
import { useEffect, useState } from "react";
import { Keyboard, Mic, ShieldCheck, ToggleRight } from "lucide-react";
import { useSettings } from "../../../../src/app/SettingsContext";
import { Badge, Button, SettingRow, SettingsGroup } from "../../../../src/ui";
import { keyboardReady, setupApi, type KeyboardRequest, type KeyboardSetup } from "./setupApi";

/** iPhone has Full Access; Android has a current keyboard instead. */
const IS_IPHONE = /iPhone|iPad|iPod/.test(navigator.userAgent);

// Android always offers a one-time grant for the microphone; it lapses soon after the app is
// left, and the keyboard then sends the user back here. Only the copy can steer them off it.
const MICROPHONE_ANDROID =
    'Choose "While using the app". "Only this time" ends a minute after you leave Simple Voice, and the keyboard asks again.';
const MICROPHONE_ANDROID_DENIED =
    'It is off. Allow opens Settings: choose "Allow only while using the app".';

function Status({ done }: { done: boolean }) {
    return done ? <Badge tone="success">Done</Badge> : <Badge tone="warning">To do</Badge>;
}

function microphoneHelp(setup: KeyboardSetup): string {
    const denied = setup.microphone === "denied";
    if (IS_IPHONE) {
        return denied
            ? "It was turned off; the button opens Settings to turn it back on."
            : "The app records what you say in the keyboard.";
    }
    return denied ? MICROPHONE_ANDROID_DENIED : MICROPHONE_ANDROID;
}

/**
 * Setting up the keyboard without leaving the app: each step shows whether the phone reports it
 * done, with a button that opens the right settings page. The status refreshes when the app
 * comes back from Settings (the shell remounts the tab on every return). A `request` from the
 * Android keyboard runs its step: the Groq key field, or the microphone prompt and, once it is
 * allowed, back to the field the keyboard was typing in.
 */
export function KeyboardSetupGroup({
    request,
    onRequest,
}: {
    request: KeyboardRequest | null;
    onRequest: (next: KeyboardRequest | null) => void;
}) {
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

    useEffect(() => {
        if (!setup || !request) return;
        if (request.step === "groqKey") {
            onRequest(null);
            document.getElementById("svm-voice")?.scrollIntoView({ behavior: "smooth" });
        } else if (setup.microphone === "granted") {
            onRequest(null);
            void setupApi.returnToKeyboard().catch(() => undefined);
        } else if (!request.asked) {
            // Asked once: a refusal, or a trip to Settings after a permanent one, does not loop.
            // A grant reads the status again, and the branch above sends the user back.
            onRequest({ step: "microphone", asked: true });
            void setupApi
                .requestMicrophone()
                .catch(() => undefined)
                .finally(() => {
                    setChecks((count) => count + 1);
                });
        }
    }, [setup, request, onRequest]);

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
                    description={microphoneHelp(setup)}
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
