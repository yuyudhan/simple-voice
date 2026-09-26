// FilePath: src/features/updates/UpdateBanner.tsx
// Shown above every page while a newer release is out. Dismissing it skips that version only:
// the next release brings it back, and Settings → System keeps showing the update meanwhile.
import { CircleArrowUp, X } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { IconButton } from "../../ui";
import { UpdateActions } from "./UpdateActions";
import { useUpdates } from "./useUpdates";
import "./updates.css";

export function UpdateBanner() {
    const { settings, update } = useSettings();
    const { status } = useUpdates();
    const latest = status?.latest;
    if (!status?.updateAvailable || !latest || latest.version === settings.skippedUpdate) {
        return null;
    }

    return (
        <div className="sv-update-banner" role="status">
            <CircleArrowUp size={16} className="sv-update-banner__icon" aria-hidden="true" />
            <p className="sv-update-banner__text">
                <strong>Simple Voice {latest.version} is available.</strong> You have{" "}
                {status.currentVersion}; update from Terminal.
            </p>
            <UpdateActions release={latest} />
            <IconButton
                label="Skip this version"
                icon={<X size={14} />}
                onClick={() => {
                    void update({ skippedUpdate: latest.version });
                }}
            />
        </div>
    );
}
