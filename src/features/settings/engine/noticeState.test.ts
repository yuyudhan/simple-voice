// FilePath: src/features/settings/engine/noticeState.test.ts
/// <reference types="bun" />
import { describe, expect, test } from "bun:test";
import { ACTION_LABEL, blamesEngine, engineNotice } from "./noticeState";

describe("engineNotice", () => {
    test("says nothing before the status loads or while the engine runs", () => {
        expect(engineNotice(null)).toBeNull();
        expect(engineNotice({ state: "running", reason: null })).toBeNull();
        expect(
            engineNotice(
                { state: "running", reason: null },
                "Apple Intelligence is turned off in System Settings.",
            ),
        ).toBeNull();
    });

    test("notes a starting engine without offering a button", () => {
        expect(engineNotice({ state: "starting", reason: null })).toEqual({
            tone: "note",
            text: "Starting the speech engine…",
            action: null,
        });
    });

    test("shows why a down engine stopped and offers to start it", () => {
        const notice = engineNotice({
            state: "down",
            reason: "The speech engine could not start: Library not loaded: @rpath/llama",
        });
        expect(notice).toEqual({
            tone: "warning",
            text:
                "The speech engine could not start: Library not loaded: @rpath/llama. " +
                "Simple Voice keeps retrying on its own.",
            action: "start",
        });
        expect(ACTION_LABEL.start).toBe("Start engine");
    });

    test("falls back to a generic reason and keeps existing punctuation", () => {
        expect(engineNotice({ state: "down", reason: null })?.text).toBe(
            "The speech engine is not running. Simple Voice keeps retrying on its own.",
        );
        expect(engineNotice({ state: "down", reason: "Stopped…" })?.text).toBe(
            "Stopped… Simple Voice keeps retrying on its own.",
        );
    });

    test("offers a restart when the engine runs but does not answer", () => {
        const notice = engineNotice(
            { state: "running", reason: null },
            "The on-device engine did not respond",
        );
        expect(notice).toEqual({
            tone: "warning",
            text: "The on-device engine did not respond",
            action: "restart",
        });
        expect(ACTION_LABEL.restart).toBe("Restart engine");
    });
});

describe("blamesEngine", () => {
    test("recognises only the backend's engine failure reasons", () => {
        expect(blamesEngine("The on-device engine is unavailable: gone")).toBe(true);
        expect(blamesEngine("Apple Intelligence requires macOS 26 or later")).toBe(false);
        expect(blamesEngine(null)).toBe(false);
        expect(blamesEngine(undefined)).toBe(false);
    });
});
