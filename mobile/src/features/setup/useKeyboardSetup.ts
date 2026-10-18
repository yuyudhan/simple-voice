// FilePath: mobile/src/features/setup/useKeyboardSetup.ts
import { useEffect, useState } from "react";
import {
    keyboardReady,
    setupApi,
    type KeyboardRequest,
    type KeyboardSetup,
    type KeyboardStep,
} from "./setupApi";

// Android's keyboard picker is a dialog over the app: the WebView gets no event when the user
// picks a keyboard there (nor from the iPhone's Settings toggles), so the status is polled
// while a step is left, as the desktop onboarding polls macOS permissions.
const POLL_MS = 1500;

function same(a: KeyboardSetup | null, b: KeyboardSetup): boolean {
    return (
        a !== null &&
        a.enabled === b.enabled &&
        a.fullAccess === b.fullAccess &&
        a.selected === b.selected &&
        a.microphone === b.microphone
    );
}

/**
 * What the phone reports about keyboard setup, read again after each action and whenever the
 * app comes back from the phone's settings, and polled while a step is left. A `request` from
 * the Android keyboard runs its step: `onStep` shows it (the Groq key field, or the microphone
 * step), and the microphone prompt is asked once; as soon as it is allowed, the app goes back
 * to the field the keyboard was typing in. Mount one user of this hook at a time, so a request
 * runs once.
 */
export function useKeyboardSetup(
    request: KeyboardRequest | null,
    onRequest: (next: KeyboardRequest | null) => void,
    onStep: (step: KeyboardStep) => void,
) {
    const [setup, setSetup] = useState<KeyboardSetup | null>(null);
    const [busy, setBusy] = useState(false);
    // Bumped to read the status again.
    const [checks, setChecks] = useState(0);

    useEffect(() => {
        let live = true;
        setupApi
            .getKeyboardSetup()
            .then((next) => {
                if (live) setSetup((current) => (same(current, next) ? current : next));
            })
            .catch(() => {
                if (live) setSetup(null);
            });
        return () => {
            live = false;
        };
    }, [checks]);

    useEffect(() => {
        const onVisible = () => {
            if (document.visibilityState === "visible") setChecks((count) => count + 1);
        };
        document.addEventListener("visibilitychange", onVisible);
        return () => {
            document.removeEventListener("visibilitychange", onVisible);
        };
    }, []);

    const pending = setup !== null && !keyboardReady(setup);
    useEffect(() => {
        if (!pending) return;
        const timer = window.setInterval(() => {
            if (document.visibilityState === "visible") setChecks((count) => count + 1);
        }, POLL_MS);
        return () => {
            window.clearInterval(timer);
        };
    }, [pending]);

    useEffect(() => {
        if (!setup || !request) return;
        if (request.step === "groqKey") {
            onRequest(null);
            onStep("groqKey");
        } else if (setup.microphone === "granted") {
            onRequest(null);
            void setupApi.returnToKeyboard().catch(() => undefined);
        } else if (!request.asked) {
            // Asked once: a refusal, or a trip to Settings after a permanent one, does not loop.
            // A grant reads the status again, and the branch above sends the user back.
            onRequest({ step: "microphone", asked: true });
            onStep("microphone");
            void setupApi
                .requestMicrophone()
                .catch(() => undefined)
                .finally(() => {
                    setChecks((count) => count + 1);
                });
        }
    }, [setup, request, onRequest, onStep]);

    const run = (action: () => Promise<unknown>) => {
        setBusy(true);
        void action()
            .catch(() => undefined)
            .finally(() => {
                setBusy(false);
                setChecks((count) => count + 1);
            });
    };

    return { setup, busy, run };
}
