// FilePath: src/features/health/HealthBanner.tsx
// One banner per issue the backend reports (no microphone, microphone access off, engine down,
// model missing, cloud key refused, ...), pinned above every page, most severe first. Each says
// in the backend's words what is wrong and offers the fixes as buttons. Banners cannot be
// dismissed: each stays until its cause is gone, and then disappears on its own.
import { useState } from "react";
import { useShell } from "../../app/ShellContext";
import { api, errorMessage, type Issue, type IssueAction } from "../../lib/api";
import { useToast } from "../../ui";
import { HealthBannerView } from "./HealthBannerView";
import { actionTarget } from "./healthState";
import { useHealth } from "./useHealth";

export function HealthBanner() {
    const health = useHealth();
    const { navigate } = useShell();
    const { toast } = useToast();
    const [busy, setBusy] = useState<string | null>(null);
    if (health === null || health.issues.length === 0) return null;

    const run = (issue: Issue, action: IssueAction) => {
        const target = actionTarget(issue, action);
        if (target.type === "page") {
            navigate(target.page);
            return;
        }
        const call: () => Promise<unknown> =
            target.type === "restart_engine"
                ? api.restartEngine
                : target.type === "download"
                  ? () => api.downloadModel(target.model)
                  : target.type === "permission"
                    ? target.request
                        ? () => api.requestPermission(target.kind)
                        : () => api.openPermissionSettings(target.kind)
                    : () => api.healthAction(action, issue.kind);
        setBusy(`${issue.kind}/${action}`);
        call()
            .catch((error: unknown) => {
                toast(errorMessage(error), "danger");
            })
            .finally(() => {
                setBusy(null);
            });
    };

    return <HealthBannerView issues={health.issues} busy={busy} onAction={run} />;
}
