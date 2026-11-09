// FilePath: src/features/health/HealthBanner.test.tsx
/// <reference types="bun" />
import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import type { Issue, IssueAction, IssueKind } from "../../lib/api";
import { HealthBannerView } from "./HealthBannerView";
import { ACTION_LABEL, actionTarget, issueTone } from "./healthState";

function issue(
    kind: IssueKind,
    actions: IssueAction[],
    severity: Issue["severity"] = "blocking",
): Issue {
    return {
        ...kind,
        severity,
        title: `Title of ${kind.kind}`,
        message: `What to do about ${kind.kind}.`,
        actions,
    };
}

/** One issue per kind, with the actions the backend offers for it (sv_domain::health::text). */
const SCENARIOS: Issue[] = [
    issue({ kind: "no_microphone" }, ["open_sound_settings", "recheck", "open_help"]),
    issue({ kind: "no_sound_server" }, ["open_help", "recheck"]),
    issue(
        {
            kind: "selected_microphone_missing",
            requested: "Shure MV7",
            device: "MacBook Pro Microphone",
        },
        ["choose_microphone"],
        "warning",
    ),
    issue(
        { kind: "microphone_disconnected", device: "AirPods Pro" },
        ["choose_microphone", "recheck"],
        "warning",
    ),
    issue(
        { kind: "microphone_no_audio", device: "USB Mic" },
        ["open_sound_settings", "choose_microphone"],
        "warning",
    ),
    issue({ kind: "microphone_silent", device: "USB Mic" }, ["open_sound_settings"], "warning"),
    issue(
        { kind: "microphone_busy", device: "USB Mic" },
        ["recheck", "choose_microphone"],
        "warning",
    ),
    issue({ kind: "microphone_failed", reason: "host error" }, ["recheck", "open_help"], "warning"),
    issue({ kind: "microphone_denied" }, ["open_microphone_privacy"]),
    issue({ kind: "microphone_not_asked" }, ["request_microphone"], "warning"),
    issue({ kind: "accessibility_denied" }, ["open_accessibility"], "warning"),
    issue({ kind: "engine_down", reason: "crashed" }, ["restart_engine"]),
    issue({ kind: "engine_not_responding", reason: "timeout" }, ["restart_engine"], "warning"),
    issue({ kind: "model_missing", model: "parakeet-tdt-v3", name: "Parakeet" }, [
        "download_model",
        "open_model_settings",
    ]),
    issue(
        { kind: "model_downloading", model: "parakeet-tdt-v3", name: "Parakeet", percent: 40 },
        [],
        "info",
    ),
    issue(
        {
            kind: "model_unavailable",
            model: "apple-speech",
            name: "Apple Speech",
            reason: "macOS 26",
        },
        ["open_model_settings"],
    ),
    issue(
        { kind: "model_failed", model: "parakeet-tdt-v3", name: "Parakeet", reason: "damaged" },
        ["download_model", "open_model_settings"],
        "warning",
    ),
    issue(
        { kind: "provider_key_missing", provider: "Groq" },
        ["open_provider_settings"],
        "warning",
    ),
    issue(
        { kind: "provider_key_invalid", provider: "Groq" },
        ["open_provider_settings"],
        "warning",
    ),
    issue({ kind: "provider_rate_limited", provider: "Groq" }, [], "info"),
    issue({ kind: "provider_offline", provider: "Groq" }, ["recheck"], "info"),
    issue(
        { kind: "shortcut_unavailable", shortcut: "Alt+/" },
        ["open_shortcut_settings"],
        "warning",
    ),
    issue({ kind: "wayland_shortcuts" }, ["open_help"], "info"),
];

function render(issues: Issue[], busy: string | null = null): string {
    return renderToStaticMarkup(
        <HealthBannerView issues={issues} busy={busy} onAction={() => undefined} />,
    );
}

describe("HealthBannerView", () => {
    test("renders nothing when all is well", () => {
        expect(render([])).toBe("");
    });

    for (const scenario of SCENARIOS) {
        test(`renders the ${scenario.kind} banner with its text and actions`, () => {
            const html = render([scenario]);
            expect(html).toContain(`data-kind="${scenario.kind}"`);
            expect(html).toContain(`sv-health-banner--${issueTone(scenario)}`);
            expect(html).toContain(scenario.title);
            expect(html).toContain(scenario.message);
            for (const action of scenario.actions) {
                expect(html).toContain(`data-action="${action}"`);
                expect(html).toContain(ACTION_LABEL[action]);
            }
            expect(html.match(/data-action=/g)?.length ?? 0).toBe(scenario.actions.length);
        });
    }

    test("alerts for problems and stays polite for notes", () => {
        const html = render(SCENARIOS);
        expect(html).toContain('role="alert" data-kind="no_microphone"');
        expect(html).toContain('role="status" data-kind="wayland_shortcuts"');
    });

    test("shows every issue, in the order given", () => {
        const html = render(SCENARIOS.slice(0, 3));
        const first = html.indexOf("no_microphone");
        const second = html.indexOf("no_sound_server");
        const third = html.indexOf("selected_microphone_missing");
        expect(first).toBeGreaterThanOrEqual(0);
        expect(first).toBeLessThan(second);
        expect(second).toBeLessThan(third);
    });

    test("marks the running action busy", () => {
        const html = render(SCENARIOS.slice(0, 1), "no_microphone/recheck");
        expect(html).toContain('aria-busy="true"');
    });
});

describe("actionTarget", () => {
    const missing = issue(
        { kind: "model_missing", model: "parakeet-tdt-v3", name: "Parakeet" },
        [],
    );
    const none = issue({ kind: "no_microphone" }, []);

    test("routes settings actions to their page", () => {
        expect(actionTarget(none, "choose_microphone")).toEqual({
            type: "page",
            page: "dictation",
        });
        expect(actionTarget(none, "open_shortcut_settings")).toEqual({
            type: "page",
            page: "dictation",
        });
        expect(actionTarget(none, "open_provider_settings")).toEqual({
            type: "page",
            page: "transcription",
        });
        expect(actionTarget(none, "open_model_settings")).toEqual({
            type: "page",
            page: "transcription",
        });
    });

    test("downloads the issue's model", () => {
        expect(actionTarget(missing, "download_model")).toEqual({
            type: "download",
            model: "parakeet-tdt-v3",
        });
    });

    test("routes permissions to the request or the settings pane", () => {
        expect(actionTarget(none, "request_microphone")).toEqual({
            type: "permission",
            kind: "microphone",
            request: true,
        });
        expect(actionTarget(none, "open_microphone_privacy")).toEqual({
            type: "permission",
            kind: "microphone",
            request: false,
        });
        expect(actionTarget(none, "open_accessibility")).toEqual({
            type: "permission",
            kind: "accessibility",
            request: false,
        });
    });

    test("hands sound settings, recheck and help to the app, and restarts the engine", () => {
        for (const action of ["open_sound_settings", "recheck", "open_help"] as const) {
            expect(actionTarget(none, action)).toEqual({ type: "app" });
        }
        expect(actionTarget(none, "restart_engine")).toEqual({ type: "restart_engine" });
    });
});
