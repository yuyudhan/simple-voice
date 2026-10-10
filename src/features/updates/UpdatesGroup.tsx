// FilePath: src/features/updates/UpdatesGroup.tsx
// Settings → App → Updates: the running version, the last check, and how to update.
import { RefreshCw } from "lucide-react";
import type { UpdateStatus } from "../../lib/api";
import { useSettings } from "../../app/SettingsContext";
import { Button, SettingRow, SettingsGroup, Toggle } from "../../ui";
import { UpdateActions } from "./UpdateActions";
import { useUpdates } from "./useUpdates";
import "./updates.css";

const relativeFormat = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });

/** "just now", "5 minutes ago", "3 hours ago", "yesterday". */
function formatAgo(ms: number, now = Date.now()): string {
    const minutes = Math.round((ms - now) / 60_000);
    if (minutes > -1) return "just now";
    if (minutes > -60) return relativeFormat.format(minutes, "minute");
    const hours = Math.round(minutes / 60);
    if (hours > -24) return relativeFormat.format(hours, "hour");
    return relativeFormat.format(Math.round(hours / 24), "day");
}

function describe(status: UpdateStatus | null, error: string | null): string {
    if (status?.installedVersion) {
        return `Version ${status.installedVersion} is installed; restart to finish updating.`;
    }
    if (status?.checking) return "Checking for updates…";
    if (status?.updateAvailable && status.latest) {
        return `Version ${status.latest.version} is available.`;
    }
    if (error !== null) return `Could not check for updates. ${error}`;
    if (typeof status?.checkedAt === "number") {
        return `Up to date. Checked ${formatAgo(status.checkedAt)}.`;
    }
    return "Not checked yet.";
}

export function UpdatesGroup() {
    const { settings, update } = useSettings();
    const { status, error, check } = useUpdates();
    const available = (status?.updateAvailable ? status.latest : null) ?? null;
    const restart = typeof status?.installedVersion === "string";
    const offer = restart || available !== null;
    // Release notes describe the version the button installs or restarts into.
    const notes = restart && available?.version !== status.installedVersion ? null : available;

    return (
        <SettingsGroup title="Updates">
            <SettingRow
                title={`Simple Voice ${status?.currentVersion ?? ""}`}
                description={
                    <span className={error !== null && !offer ? "sv-update-error" : undefined}>
                        {describe(status, error)}
                    </span>
                }
            >
                <Button
                    size="sm"
                    icon={<RefreshCw size={13} />}
                    loading={status?.checking === true}
                    onClick={() => {
                        void check();
                    }}
                >
                    Check now
                </Button>
            </SettingRow>
            {offer && status && (
                <SettingRow
                    title={restart ? "Restart to update" : "Install update"}
                    description={
                        status.installError !== null && !status.installing ? (
                            <span className="sv-update-error">{status.installError}</span>
                        ) : restart ? (
                            "The new version is already installed. Simple Voice quits and reopens on it."
                        ) : (
                            "Downloads the release from GitHub, checks its checksum and signature, and replaces the app. Simple Voice quits while it updates and reopens afterwards."
                        )
                    }
                >
                    <UpdateActions
                        release={notes}
                        installing={status.installing}
                        restart={restart}
                    />
                </SettingRow>
            )}
            <SettingRow
                title="Check for updates automatically"
                description="Once a day, Simple Voice asks GitHub for its latest release. Nothing about you or your dictations is sent."
            >
                <Toggle
                    label="Check for updates automatically"
                    checked={settings.checkForUpdates}
                    onChange={(checkForUpdates) => {
                        void update({ checkForUpdates });
                    }}
                />
            </SettingRow>
        </SettingsGroup>
    );
}
