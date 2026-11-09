// FilePath: src/features/health/HealthBannerView.tsx
// The banners for a list of issues, without wiring (no events, commands or toasts), so tests can
// render every one. Button comes from its own module: the ui barrel also loads Modal, which
// needs a DOM at import time.
import { CircleAlert, Info, TriangleAlert } from "lucide-react";
import type { Issue, IssueAction } from "../../lib/api";
import { Button } from "../../ui/Button";
import { ACTION_LABEL, issueTone } from "./healthState";
import "./health.css";

const ICON = { danger: CircleAlert, warning: TriangleAlert, note: Info } as const;

interface Props {
    issues: Issue[];
    /** `kind/action` of the button whose action is running. */
    busy: string | null;
    onAction: (issue: Issue, action: IssueAction) => void;
}

export function HealthBannerView({ issues, busy, onAction }: Props) {
    return (
        <>
            {issues.map((issue) => {
                const tone = issueTone(issue);
                const Icon = ICON[tone];
                return (
                    <div
                        key={issue.kind}
                        className={`sv-health-banner sv-health-banner--${tone}`}
                        role={tone === "note" ? "status" : "alert"}
                        data-kind={issue.kind}
                    >
                        <Icon size={16} className="sv-health-banner__icon" aria-hidden="true" />
                        <p className="sv-health-banner__text">
                            <strong>{issue.title}.</strong> {issue.message}
                        </p>
                        {issue.actions.length > 0 && (
                            <div className="sv-health-banner__actions">
                                {issue.actions.map((action, index) => (
                                    <Button
                                        key={action}
                                        size="sm"
                                        variant={index === 0 ? "primary" : "secondary"}
                                        loading={busy === `${issue.kind}/${action}`}
                                        data-action={action}
                                        onClick={() => {
                                            onAction(issue, action);
                                        }}
                                    >
                                        {ACTION_LABEL[action]}
                                    </Button>
                                ))}
                            </div>
                        )}
                    </div>
                );
            })}
        </>
    );
}
