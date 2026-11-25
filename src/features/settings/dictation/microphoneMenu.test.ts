// FilePath: src/features/settings/dictation/microphoneMenu.test.ts
/// <reference types="bun" />
import { describe, expect, test } from "bun:test";
import type { Microphone } from "../../../lib/api";
import { BUILT_IN_OR_DEFAULT, microphoneMenu } from "./microphoneMenu";

function mic(name: string, flags: Partial<Microphone> = {}): Microphone {
    return { id: name, name, isDefault: false, isBuiltIn: false, ...flags };
}

const HEADPHONES = mic("WH-XB910N", { isDefault: true });
const BUILT_IN = mic("MacBook Pro Microphone", { isBuiltIn: true });

describe("microphoneMenu", () => {
    test("offers no Automatic entry", () => {
        const menu = microphoneMenu([HEADPHONES, BUILT_IN], null, false);
        expect(menu.options.some((o) => /automatic/i.test(o.label))).toBe(false);
    });

    test("a fresh setting selects the built-in microphone, not the Bluetooth default", () => {
        const menu = microphoneMenu([HEADPHONES, BUILT_IN], null, false);
        expect(menu.value).toBe(BUILT_IN_OR_DEFAULT);
        expect(menu.options).toEqual([
            { value: BUILT_IN_OR_DEFAULT, label: "MacBook Pro Microphone (built-in)" },
            { value: "WH-XB910N", label: "WH-XB910N" },
        ]);
        expect(menu.note).toBeNull();
    });

    test("the built-in microphone stored by name shows as the built-in entry", () => {
        const menu = microphoneMenu([HEADPHONES, BUILT_IN], "MacBook Pro Microphone", false);
        expect(menu.value).toBe(BUILT_IN_OR_DEFAULT);
        expect(menu.options).toHaveLength(2);
    });

    test("an explicit choice is kept", () => {
        const menu = microphoneMenu([HEADPHONES, BUILT_IN], "WH-XB910N", false);
        expect(menu.value).toBe("WH-XB910N");
    });

    test("a chosen device that went away stays chosen and shows as not connected", () => {
        const menu = microphoneMenu([BUILT_IN], "WH-XB910N", false);
        expect(menu.value).toBe("WH-XB910N");
        expect(menu.options).toContainEqual({
            value: "WH-XB910N",
            label: "WH-XB910N (not connected)",
        });
    });

    test("a Mac without a built-in microphone falls back to the system default, clearly", () => {
        const menu = microphoneMenu([HEADPHONES, mic("Shure MV7")], null, false);
        expect(menu.value).toBe(BUILT_IN_OR_DEFAULT);
        expect(menu.options[0]).toEqual({
            value: BUILT_IN_OR_DEFAULT,
            label: "System default (WH-XB910N)",
        });
        expect(menu.options.map((o) => o.value)).toEqual([
            BUILT_IN_OR_DEFAULT,
            "WH-XB910N",
            "Shure MV7",
        ]);
        expect(menu.note).toBe(
            "This Mac has no built-in microphone, so Simple Voice records from the system default input.",
        );
    });

    test("Linux follows the desktop's default input without claiming a missing microphone", () => {
        const menu = microphoneMenu([mic("Default Audio Device", { isDefault: true })], null, true);
        expect(menu.options[0]?.label).toBe("System default (Default Audio Device)");
        expect(menu.note).toBeNull();
    });

    test("before the list loads the stored null reads as the built-in microphone", () => {
        const menu = microphoneMenu(null, null, false);
        expect(menu.options).toEqual([
            { value: BUILT_IN_OR_DEFAULT, label: "Built-in microphone" },
        ]);
        expect(menu.note).toBeNull();
    });
});
