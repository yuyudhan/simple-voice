// FilePath: src/features/settings/engine/EngineNotice.tsx
// The speech engine's state beside an on-device feature, with the button that starts it when it
// is down or restarts it when it stops answering. Renders nothing while the engine is healthy.
import { Button } from "../../../ui";
import { ACTION_LABEL, engineNotice } from "./noticeState";
import type { EngineStatusState } from "./useEngineStatus";
import "../common.css";
import "./engine.css";

export function EngineNotice({
    engine,
    featureReason = null,
}: {
    engine: EngineStatusState;
    featureReason?: string | null;
}) {
    const notice = engineNotice(engine.status, featureReason);
    if (notice === null && engine.error === null) return null;
    return (
        <div className="sv-engine-notice">
            {notice && (
                <p
                    className={notice.tone === "warning" ? "sv-inline-warning" : "sv-inline-note"}
                    role="status"
                >
                    {notice.text}
                </p>
            )}
            {notice?.action && (
                <div>
                    <Button
                        variant="secondary"
                        size="sm"
                        loading={engine.restarting}
                        onClick={() => {
                            void engine.restart();
                        }}
                    >
                        {ACTION_LABEL[notice.action]}
                    </Button>
                </div>
            )}
            {engine.error && (
                <p className="sv-inline-error" role="alert">
                    {engine.error}
                </p>
            )}
        </div>
    );
}
