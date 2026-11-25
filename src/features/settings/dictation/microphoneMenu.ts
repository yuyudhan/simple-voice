// FilePath: src/features/settings/dictation/microphoneMenu.ts
// The Input device menu. The stored `null` is the Mac's built-in microphone (sv-audio's
// `choose_input`), so that entry is named after it; connecting a Bluetooth headset never moves
// dictation to it. A Mac without one (Mac mini, Mac Studio) and Linux record from the system
// default input instead, and the entry says so. Any other device is an explicit choice that is
// kept until the user changes it.
import type { Microphone } from "../../../lib/api";

/** Select values are strings; the empty string stands for the stored `null`. */
export const BUILT_IN_OR_DEFAULT = "";

export interface MicrophoneOption {
    value: string;
    label: string;
}

export interface MicrophoneMenu {
    value: string;
    options: MicrophoneOption[];
    /** Shown under the menu when the Mac has no built-in microphone to prefer. */
    note: string | null;
}

const NO_BUILT_IN =
    "This Mac has no built-in microphone, so Simple Voice records from the system default input.";

/** `microphones` is `null` until the device list has loaded. */
export function microphoneMenu(
    microphones: Microphone[] | null,
    current: string | null,
    linux: boolean,
): MicrophoneMenu {
    if (microphones === null) {
        const options = [{ value: BUILT_IN_OR_DEFAULT, label: "Built-in microphone" }];
        if (current !== null) options.push({ value: current, label: current });
        return { value: current ?? BUILT_IN_OR_DEFAULT, options, note: null };
    }

    const builtIn = microphones.find((m) => m.isBuiltIn);
    const systemDefault = microphones.find((m) => m.isDefault);
    const first: MicrophoneOption = builtIn
        ? { value: BUILT_IN_OR_DEFAULT, label: `${builtIn.name} (built-in)` }
        : {
              value: BUILT_IN_OR_DEFAULT,
              label: systemDefault ? `System default (${systemDefault.name})` : "System default",
          };
    const options = [
        first,
        ...microphones.filter((m) => m !== builtIn).map((m) => ({ value: m.id, label: m.name })),
    ];

    // The built-in microphone stored by name (an older explicit pick) is the same choice.
    const value = current === null || current === builtIn?.id ? BUILT_IN_OR_DEFAULT : current;
    if (value !== BUILT_IN_OR_DEFAULT && !microphones.some((m) => m.id === value)) {
        options.push({ value, label: `${value} (not connected)` });
    }

    return { value, options, note: builtIn || linux ? null : NO_BUILT_IN };
}
