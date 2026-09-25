// FilePath: src/features/updates/UpdateActions.tsx
// Homebrew installs every update, so the app hands the user the one command that does it. It
// does not run brew itself: the cask quits the running app while it upgrades, which would kill
// an upgrade the app started, and a GUI app cannot rely on finding brew on its PATH.
import { Copy, ExternalLink } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorMessage, type Release } from "../../lib/api";
import { Button, useToast } from "../../ui";

export const UPGRADE_COMMAND = "brew upgrade --cask simple-voice";

export function UpdateActions({ release }: { release: Release }) {
    const { toast } = useToast();

    const copy = async () => {
        try {
            await api.copyText(UPGRADE_COMMAND);
            toast("Command copied. Paste it into Terminal to update.", "success");
        } catch (e) {
            toast(errorMessage(e), "danger");
        }
    };

    return (
        <div className="sv-update-actions">
            <Button
                variant="primary"
                size="sm"
                icon={<Copy size={13} />}
                onClick={() => {
                    void copy();
                }}
            >
                Copy update command
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
