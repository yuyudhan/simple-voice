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

export const setupApi = {
    getKeyboardSetup: () => invoke<KeyboardSetup>("get_keyboard_setup"),
    openKeyboardSettings: () => invoke<null>("open_keyboard_settings"),
    chooseKeyboard: () => invoke<null>("choose_keyboard"),
    requestMicrophone: () => invoke<MicrophoneStatus>("request_microphone"),
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
