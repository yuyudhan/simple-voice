// FilePath: src/features/updates/UpdateActions.tsx
// "Install update" runs the published install script, the README's curl command: it installs
// straight from the GitHub release. The script quits the app, replaces it and reopens it;
// progress and failures arrive through `update-status`.
import { Download, ExternalLink } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorMessage, type Release } from "../../lib/api";
import { Button, useToast } from "../../ui";

export function UpdateActions({ release, installing }: { release: Release; installing: boolean }) {
    const { toast } = useToast();

    const install = async () => {
        try {
            await api.installUpdate();
        } catch (e) {
            toast(errorMessage(e), "danger");
        }
    };

    return (
        <div className="sv-update-actions">
            <Button
                variant="primary"
                size="sm"
                icon={<Download size={13} />}
                loading={installing}
                onClick={() => {
                    void install();
                }}
            >
                {installing ? "Installing…" : "Install update"}
            </Button>
            <Button
                variant="ghost"
                size="sm"
                icon={<ExternalLink size={13} />}
                onClick={() => {
                    void openUrl(release.url).catch((e: unknown) => {
                        toast(errorMessage(e), "danger");
                    });
                }}
            >
                Release notes
            </Button>
        </div>
    );
}
