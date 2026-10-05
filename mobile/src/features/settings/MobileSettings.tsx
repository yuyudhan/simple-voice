// FilePath: mobile/src/features/settings/MobileSettings.tsx
import { Monitor, Moon, Sun } from "lucide-react";
import { useSettings } from "../../../../src/app/SettingsContext";
import { GroqKeyField } from "../../../../src/features/settings/models/GroqKeyField";
import { StylePage } from "../../../../src/features/style/StylePage";
import type { Theme } from "../../../../src/lib/api";
import { Segmented, SettingRow, SettingsGroup, type SegmentedOption } from "../../../../src/ui";
import "../../../../src/features/settings/models/models.css";

const THEME_OPTIONS: SegmentedOption<Theme>[] = [
    { value: "system", label: "System", icon: <Monitor /> },
    { value: "light", label: "Light", icon: <Sun /> },
    { value: "dark", label: "Dark", icon: <Moon /> },
];

/** The Groq key the keyboard needs, Style (shared with the desktop app) and appearance. */
export function MobileSettings() {
    const { settings, update } = useSettings();
    return (
        <>
            <StylePage />
            <div className="svm-settings__group">
                <SettingsGroup title="Voice">
                    <SettingRow
                        title="Groq"
                        description="The Simple Voice keyboard sends what you say to Groq to transcribe and format it. One key does both."
                    />
                    <div className="sv-models-section__key">
                        <GroqKeyField />
                    </div>
                </SettingsGroup>
            </div>
            <div className="svm-settings__group">
                <SettingsGroup title="Appearance">
                    <SettingRow title="Theme" description="System follows your phone's appearance.">
                        <Segmented
                            label="Theme"
                            value={settings.theme}
                            options={THEME_OPTIONS}
                            onChange={(theme) => {
                                void update({ theme });
                            }}
                        />
                    </SettingRow>
                </SettingsGroup>
            </div>
        </>
    );
}
