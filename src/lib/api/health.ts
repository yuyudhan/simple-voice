// FilePath: src/lib/api/health.ts
// Wire types of the app's health (`sv_domain::health`): `get_health`, `health_action` and the
// `health-changed` event.

/** How much an issue gets in the way; `blocking` refuses a dictation with its reason. */
export type Severity = "blocking" | "warning" | "info";

/** What the user can do about an issue, offered as buttons in the order given. */
export type IssueAction =
    | "open_sound_settings"
    | "open_microphone_privacy"
    | "request_microphone"
    | "open_accessibility"
    | "choose_microphone"
    | "recheck"
    | "restart_engine"
    | "download_model"
    | "open_model_settings"
    | "open_provider_settings"
    | "open_shortcut_settings"
    | "open_help";

/** Every condition the app reports, with the fields its actions need (`sv_domain::IssueKind`). */
export type IssueKind =
    | { kind: "no_microphone" }
    | { kind: "no_sound_server" }
    | { kind: "selected_microphone_missing"; requested: string; device: string }
    | { kind: "microphone_disconnected"; device: string }
    | { kind: "microphone_no_audio"; device: string }
    | { kind: "microphone_silent"; device: string }
    | { kind: "microphone_busy"; device: string }
    | { kind: "microphone_failed"; reason: string }
    | { kind: "microphone_denied" }
    | { kind: "microphone_not_asked" }
    | { kind: "accessibility_denied" }
    | { kind: "engine_down"; reason: string }
    | { kind: "engine_not_responding"; reason: string }
    | { kind: "model_missing"; model: string; name: string }
    | { kind: "model_downloading"; model: string; name: string; percent: number }
    | { kind: "model_unavailable"; model: string; name: string; reason: string }
    | { kind: "model_failed"; model: string; name: string; reason: string }
    | { kind: "provider_key_missing"; provider: string }
    | { kind: "provider_key_invalid"; provider: string }
    | { kind: "provider_rate_limited"; provider: string }
    | { kind: "provider_offline"; provider: string }
    | { kind: "shortcut_unavailable"; shortcut: string }
    | { kind: "wayland_shortcuts" };

/** One condition, with the words the backend chose for it; the UI shows them as they are. */
export type Issue = IssueKind & {
    severity: Severity;
    title: string;
    message: string;
    actions: IssueAction[];
};

/** Everything wrong right now, most severe first (`health-changed`); empty when all is well. */
export interface Health {
    issues: Issue[];
}
