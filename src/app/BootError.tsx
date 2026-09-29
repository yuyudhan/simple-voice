// FilePath: src/app/BootError.tsx
// Shown when settings cannot load, typically because the database was migrated by a newer
// build. Installing the latest release is the fix for that, so the screen offers it directly;
// the update commands need no database.
import { useState } from "react";
import { Download } from "lucide-react";
import { api, errorMessage } from "../lib/api";
import { useUpdates } from "../features/updates/useUpdates";
import { Button } from "../ui";

export function BootError({ message, onRetry }: { message: string; onRetry: () => void }) {
    const { status } = useUpdates();
    const [busy, setBusy] = useState(false);
    const [problem, setProblem] = useState<string | null>(null);

    const update = async () => {
        setBusy(true);
        setProblem(null);
        try {
            const checked = await api.checkForUpdates();
            if (checked.error !== null) {
                setProblem(`Could not check for updates: ${checked.error}`);
            } else if (!checked.updateAvailable) {
                setProblem(
                    `Simple Voice ${checked.currentVersion} is the latest release; ` +
                        "there is nothing newer to install.",
                );
            } else {
                await api.installUpdate();
            }
        } catch (e) {
            setProblem(errorMessage(e));
        } finally {
            setBusy(false);
        }
    };

    const installing = status?.installing ?? false;
    const notice = installing
        ? `Installing Simple Voice ${status?.latest?.version ?? ""}; it restarts when it is done.`
        : (problem ?? status?.installError ?? null);

    return (
        <div className="sv-boot" role="alert">
            <h1 className="sv-boot__title">Simple Voice could not start</h1>
            <p className="sv-boot__message selectable">{message}</p>
            <div className="sv-boot__actions">
                <Button
                    variant="primary"
                    icon={<Download size={13} />}
                    loading={busy || installing}
                    onClick={() => {
                        void update();
                    }}
                >
                    {installing ? "Installing…" : "Update Simple Voice"}
                </Button>
                <Button onClick={onRetry} disabled={busy || installing}>
                    Try again
                </Button>
            </div>
            {notice !== null && <p className="sv-boot__notice selectable">{notice}</p>}
        </div>
    );
}
