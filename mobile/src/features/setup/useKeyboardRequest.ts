// FilePath: mobile/src/features/setup/useKeyboardRequest.ts
import { useEffect, useState } from "react";
import { setupApi, type KeyboardRequest } from "./setupApi";

/** Only the Android keyboard opens the app with a request; the iPhone one starts a session. */
const IS_ANDROID = navigator.userAgent.includes("Android");

/**
 * The setup step the Android keyboard opened the app for (a tap on its status line), picked up
 * at launch and on every return to the foreground; `onRequest` runs for each new one. The
 * request outlives the tab remounts the shell does, so Settings can finish it across a trip to
 * the phone's settings.
 */
export function useKeyboardRequest(onRequest: () => void) {
    const [request, setRequest] = useState<KeyboardRequest | null>(null);
    useEffect(() => {
        if (!IS_ANDROID) return;
        let live = true;
        const take = () => {
            if (document.visibilityState !== "visible") return;
            setupApi
                .takeKeyboardRequest()
                .then((step) => {
                    if (!live || step === null) return;
                    setRequest(step === "microphone" ? { step, asked: false } : { step });
                    onRequest();
                })
                .catch(() => undefined);
        };
        take();
        document.addEventListener("visibilitychange", take);
        return () => {
            live = false;
            document.removeEventListener("visibilitychange", take);
        };
    }, [onRequest]);
    return [request, setRequest] as const;
}
