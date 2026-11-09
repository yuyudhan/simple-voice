// FilePath: src/features/health/healthState.ts
// What each health action button says and does, free of React so the mapping is testable on its
// own. The backend decides which issues exist, their words and their actions; the window only
// labels the buttons and routes each one to the page, command or permission it names.
import type { Issue, IssueAction, PermissionKind } from "../../lib/api";
import type { Page } from "../../app/ShellContext";

export const ACTION_LABEL: Record<IssueAction, string> = {
    open_sound_settings: "Open Sound Settings",
    open_microphone_privacy: "Open Microphone Settings",
    request_microphone: "Allow Microphone",
    open_accessibility: "Open Accessibility Settings",
    choose_microphone: "Choose Microphone",
    recheck: "Check Again",
    restart_engine: "Restart Engine",
    download_model: "Download Model",
    open_model_settings: "Choose Model",
    open_provider_settings: "Open Provider Settings",
    open_shortcut_settings: "Change Shortcut",
    open_help: "Help",
};

/** Where an action button goes. */
export type ActionTarget =
    | { type: "page"; page: Page }
    | { type: "app" }
    | { type: "restart_engine" }
    | { type: "download"; model: string }
    | { type: "permission"; kind: PermissionKind; request: boolean };

/** Exhaustive over `IssueAction`: a new action fails to type-check until it is routed here. */
export function actionTarget(issue: Issue, action: IssueAction): ActionTarget {
    switch (action) {
        case "choose_microphone":
        case "open_shortcut_settings":
            return { type: "page", page: "dictation" };
        case "open_model_settings":
        case "open_provider_settings":
            return { type: "page", page: "transcription" };
        case "restart_engine":
            return { type: "restart_engine" };
        case "download_model":
            // Only model issues offer a download; anything else falls back to the model list.
            return "model" in issue
                ? { type: "download", model: issue.model }
                : { type: "page", page: "transcription" };
        case "request_microphone":
            return { type: "permission", kind: "microphone", request: true };
        case "open_microphone_privacy":
            return { type: "permission", kind: "microphone", request: false };
        case "open_accessibility":
            return { type: "permission", kind: "accessibility", request: false };
        case "open_sound_settings":
        case "recheck":
        case "open_help":
            return { type: "app" };
    }
}

/** Banner tone: a blocking issue stops dictation; the rest are warnings or notes. */
export function issueTone(issue: Issue): "danger" | "warning" | "note" {
    if (issue.severity === "blocking") return "danger";
    if (issue.severity === "warning") return "warning";
    return "note";
}
