// FilePath: mobile/src/features/setup/setupSteps.ts
// The setup steps in the order a new user takes them. The Settings checklist and the
// first-launch guide both read this list, so they show the same copy and the same live state.
import { Keyboard, KeyRound, Mic, ShieldCheck, ToggleRight, type LucideIcon } from "lucide-react";
import { setupApi, type KeyboardSetup } from "./setupApi";

/** iPhone has Full Access; Android has a current keyboard instead. */
const IS_IPHONE = /iPhone|iPad|iPod/.test(navigator.userAgent);

// Android always offers a one-time grant for the microphone; it lapses soon after the app is
// left, and the keyboard then sends the user back here. Only the copy can steer them off it.
const MICROPHONE_ANDROID =
    'Choose "While using the app". "Only this time" ends a minute after you leave Simple Voice, and the keyboard asks again.';
const MICROPHONE_ANDROID_DENIED =
    'It is off. Allow opens Settings: choose "Allow only while using the app".';

export type SetupStepId = "groqKey" | "keyboard" | "fullAccess" | "selected" | "microphone";

export interface SetupStep {
    id: SetupStepId;
    icon: LucideIcon;
    title: string;
    description: string;
    /** What the phone (or, for the Groq key, the settings) reports now. */
    done: boolean;
    /** The phone action that does the step; the Groq key has none, it is typed into a field. */
    action: { label: string; run: () => Promise<unknown>; disabled: boolean } | null;
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
 * Groq key, keyboard added, Full Access (iPhone) or current keyboard (Android), microphone.
 * Without a `setup` (not on a phone, or the phone did not answer) only the Groq key is left.
 */
export function setupSteps(setup: KeyboardSetup | null, groqKeyPresent: boolean): SetupStep[] {
    const steps: SetupStep[] = [
        {
            id: "groqKey",
            icon: KeyRound,
            title: "Add your Groq key",
            description: "Dictation sends your voice to Groq to transcribe and format it.",
            done: groqKeyPresent,
            action: null,
        },
    ];
    if (!setup) return steps;
    const openSettings = { label: "Open settings", run: setupApi.openKeyboardSettings };
    steps.push({
        id: "keyboard",
        icon: Keyboard,
        title: "Add the Simple Voice keyboard",
        description: IS_IPHONE
            ? "Opens Simple Voice in Settings: tap Keyboards, then turn on Simple Voice."
            : "Opens On-screen keyboard settings: turn on Simple Voice.",
        done: setup.enabled,
        action: { ...openSettings, disabled: false },
    });
    if (setup.fullAccess !== null) {
        steps.push({
            id: "fullAccess",
            icon: ShieldCheck,
            title: "Allow Full Access",
            description:
                "The keyboard needs it to send your voice to the app. It turns on in the same Keyboards page.",
            done: setup.fullAccess,
            action: { ...openSettings, disabled: false },
        });
    }
    if (setup.selected !== null) {
        steps.push({
            id: "selected",
            icon: ToggleRight,
            title: "Switch to Simple Voice",
            description: "Makes it the keyboard that opens when you tap a text field.",
            done: setup.selected,
            action: {
                label: "Choose keyboard",
                run: setupApi.chooseKeyboard,
                disabled: !setup.enabled,
            },
        });
    }
    steps.push({
        id: "microphone",
        icon: Mic,
        title: "Allow the microphone",
        description: microphoneHelp(setup),
        done: setup.microphone === "granted",
        action: { label: "Allow", run: setupApi.requestMicrophone, disabled: false },
    });
    return steps;
}
