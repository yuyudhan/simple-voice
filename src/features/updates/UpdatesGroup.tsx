// FilePath: src/features/updates/UpdatesGroup.tsx
// Settings → System → Updates: the running version, the last check, and how to update.
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
    const latest = status?.updateAvailable ? status.latest : null;

    return (
        <SettingsGroup title="Updates">
            <SettingRow
                title={`Simple Voice ${status?.currentVersion ?? ""}`}
                description={
                    <span className={error !== null && !latest ? "sv-update-error" : undefined}>
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
            {latest && (
                <SettingRow
                    title="Update from Terminal"
                    description="Copy the update command and run it in Terminal. It updates through Homebrew when Homebrew installed Simple Voice, and straight from GitHub otherwise. It quits Simple Voice while it updates and reopens it afterwards."
                >
                    <UpdateActions release={latest} />
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
