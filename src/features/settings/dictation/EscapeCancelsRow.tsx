// FilePath: src/features/settings/dictation/EscapeCancelsRow.tsx
import { useSettings } from "../../../app/SettingsContext";
import { SettingRow, Toggle } from "../../../ui";

const TITLE = "Esc cancels recording";

/** Opt-in because a global Esc takes the key from Vim and dialogs for the whole recording. */
export function EscapeCancelsRow() {
    const { settings, update } = useSettings();
    return (
        <SettingRow
            title={TITLE}
            description="While recording, Esc discards the dictation instead of reaching the app you are typing in."
        >
            <Toggle
                label={TITLE}
                checked={settings.escapeCancels}
                onChange={(escapeCancels) => {
                    void update({ escapeCancels });
                }}
            />
        </SettingRow>
    );
}
