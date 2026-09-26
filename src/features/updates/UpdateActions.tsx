// FilePath: src/features/updates/UpdateActions.tsx
// The installer script updates the app: through Homebrew when Homebrew manages it and works,
// otherwise straight from the GitHub release. The app hands the user the one command that runs
// it rather than running it itself: the installer quits the running app, which would kill an
// update the app started.
import { Copy, ExternalLink } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errorMessage, type Release } from "../../lib/api";
import { Button, useToast } from "../../ui";

const UPGRADE_COMMAND =
    "curl -fsSL https://github.com/yuyudhan/simple-voice/releases/latest/download/install.sh | bash";

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
