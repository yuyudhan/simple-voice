// FilePath: mobile/src/features/setup/setupApi.ts
// The phone-only setup commands (mobile/src-tauri/src/features/setup); the desktop app has none.
import { invoke } from "@tauri-apps/api/core";

export type MicrophoneStatus = "granted" | "denied" | "undetermined";

export interface KeyboardSetup {
    /** The Simple Voice keyboard is added in the phone's keyboard settings. */
    enabled: boolean;
    /** iPhone only: Full Access is on. */
    fullAccess: boolean | null;
    /** Android only: Simple Voice is the keyboard that opens in text fields. */
    selected: boolean | null;
    microphone: MicrophoneStatus;
}

/** Android: the setup step the keyboard opened the app for, from a tap on its status line. */
export type KeyboardStep = "microphone" | "groqKey";

/** A keyboard request being handled; `asked` once the microphone prompt or Settings was shown. */
export type KeyboardRequest = { step: "microphone"; asked: boolean } | { step: "groqKey" };

export const setupApi = {
    getKeyboardSetup: () => invoke<KeyboardSetup>("get_keyboard_setup"),
    openKeyboardSettings: () => invoke<null>("open_keyboard_settings"),
    chooseKeyboard: () => invoke<null>("choose_keyboard"),
    requestMicrophone: () => invoke<MicrophoneStatus>("request_microphone"),
    takeKeyboardRequest: () => invoke<KeyboardStep | null>("take_keyboard_request"),
    returnToKeyboard: () => invoke<null>("return_to_keyboard"),
};

/** Every step the phone checks is done (the Groq key is checked separately, from settings). */
export function keyboardReady(setup: KeyboardSetup): boolean {
    return (
        setup.enabled &&
        setup.fullAccess !== false &&
        setup.selected !== false &&
        setup.microphone === "granted"
    );
}
