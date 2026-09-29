// FilePath: src/features/updates/UpdateBanner.tsx
// Shown above every page while an update needs the user: a newer build already on disk that a
// restart finishes, or a newer release to install. Dismissing an available release skips that
// version only: the next release brings it back, and Settings → App keeps showing the update
// meanwhile. After an update the banner says it worked until dismissed.
import { CircleArrowUp, CircleCheck, X } from "lucide-react";
import { useSettings } from "../../app/SettingsContext";
import { api, errorMessage, type UpdateStatus } from "../../lib/api";
import { IconButton, useToast } from "../../ui";
import { UpdateActions } from "./UpdateActions";
import { useUpdates } from "./useUpdates";
import "./updates.css";

function InstallError({ status }: { status: UpdateStatus }) {
    if (status.installError === null || status.installing) return null;
    return <span className="sv-update-error"> {status.installError}</span>;
}

export function UpdateBanner() {
    const { settings, update } = useSettings();
    const { status } = useUpdates();
    const { toast } = useToast();
    if (!status) return null;
    const latest = status.latest;

    if (status.installedVersion !== null) {
        return (
            <div className="sv-update-banner" role="status">
                <CircleArrowUp size={16} className="sv-update-banner__icon" aria-hidden="true" />
                <p className="sv-update-banner__text">
                    <strong>Simple Voice {status.installedVersion} is installed.</strong> Restart
                    Simple Voice to finish updating.
                    <InstallError status={status} />
                </p>
                <UpdateActions
                    release={latest?.version === status.installedVersion ? latest : null}
                    installing={status.installing}
                    restart
                />
            </div>
        );
    }

    if (status.updateAvailable && latest && latest.version !== settings.skippedUpdate) {
        return (
            <div className="sv-update-banner" role="status">
                <CircleArrowUp size={16} className="sv-update-banner__icon" aria-hidden="true" />
                <p className="sv-update-banner__text">
                    <strong>Simple Voice {latest.version} is available.</strong>{" "}
                    {status.installing
                        ? "Installing; Simple Voice restarts when it is done."
                        : `You have ${status.currentVersion}.`}
                    <InstallError status={status} />
                </p>
                <UpdateActions release={latest} installing={status.installing} restart={false} />
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

    if (status.updatedFrom !== null) {
        return (
            <div className="sv-update-banner" role="status">
                <CircleCheck size={16} className="sv-update-banner__icon" aria-hidden="true" />
                <p className="sv-update-banner__text">
                    <strong>Simple Voice is updated to {status.currentVersion}.</strong> You had{" "}
                    {status.updatedFrom}; your history, dictionary and settings are unchanged.
                </p>
                <IconButton
                    label="Dismiss"
                    icon={<X size={14} />}
                    onClick={() => {
                        api.dismissUpdateNotice().catch((e: unknown) => {
                            toast(errorMessage(e), "danger");
                        });
                    }}
                />
            </div>
        );
    }

    return null;
}
