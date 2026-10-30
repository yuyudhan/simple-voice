// FilePath: src/features/settings/engine/noticeState.ts
// What Settings says about the speech engine next to an on-device feature: nothing while it
// runs, a note while it starts, and the reason with a way to start it when it is down. Kept
// free of React so the decision is testable on its own.
import type { EngineStatus } from "../../../lib/api";

export type EngineAction = "start" | "restart";

export interface EngineNotice {
    tone: "note" | "warning";
    text: string;
    action: EngineAction | null;
}

/** Prefix of the model reasons the backend writes when the engine fails to answer. */
const ENGINE_FAILURE_PREFIX = "The on-device engine";

export const ACTION_LABEL: Record<EngineAction, string> = {
    start: "Start engine",
    restart: "Restart engine",
};

/** Whether a feature's unavailable reason is the engine failing to answer. */
export function blamesEngine(reason: string | null | undefined): boolean {
    return reason?.startsWith(ENGINE_FAILURE_PREFIX) ?? false;
}

/**
 * `status` is null until it loads. `featureReason` is the unavailable reason of the feature the
 * notice sits next to (a model's `reason`); it counts only when it blames the engine, which
 * means the engine runs but does not answer.
 */
export function engineNotice(
    status: EngineStatus | null,
    featureReason: string | null = null,
): EngineNotice | null {
    if (status === null) return null;
    if (status.state === "starting") {
        return { tone: "note", text: "Starting the speech engine…", action: null };
    }
    if (status.state === "down") {
        const reason = status.reason ?? "The speech engine is not running";
        const sentence = /[.!?…]$/.test(reason) ? reason : `${reason}.`;
        return {
            tone: "warning",
            text: `${sentence} Simple Voice keeps retrying on its own.`,
            action: "start",
        };
    }
    if (featureReason !== null && blamesEngine(featureReason)) {
        return { tone: "warning", text: featureReason, action: "restart" };
    }
    return null;
}
