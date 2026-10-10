// FilePath: mobile/src/features/settings/MobileSettings.tsx
import { Monitor, Moon, Sun } from "lucide-react";
import { useSettings } from "../../../../src/app/SettingsContext";
import { GroqKeyField } from "../../../../src/features/settings/models/GroqKeyField";
import { StylePage } from "../../../../src/features/style/StylePage";
import type { Theme } from "../../../../src/lib/api";
import {
    Segmented,
    Select,
    SettingRow,
    SettingsGroup,
    Toggle,
    type SegmentedOption,
    type SelectOption,
} from "../../../../src/ui";
import "../../../../src/features/settings/models/models.css";

const THEME_OPTIONS: SegmentedOption<Theme>[] = [
    { value: "system", label: "System", icon: <Monitor /> },
    { value: "light", label: "Light", icon: <Sun /> },
    { value: "dark", label: "Dark", icon: <Moon /> },
];

const SESSION_OPTIONS: SelectOption<string>[] = [5, 10, 15, 30, 60].map((minutes) => ({
    value: String(minutes),
    label: `${String(minutes)} minutes`,
}));

/** Only the iPhone keyboard needs a session and can transcribe offline. */
const IS_IPHONE = /iPhone|iPad|iPod/.test(navigator.userAgent);

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
                    {IS_IPHONE && (
                        <>
                            <SettingRow
                                title="Apple Speech when offline"
                                description="Without internet, transcribe on this iPhone. Text gets the rules only, no AI formatting."
                            >
                                <Toggle
                                    label="Apple Speech when offline"
                                    checked={settings.appleSpeechOffline}
                                    onChange={(appleSpeechOffline) => {
                                        void update({ appleSpeechOffline });
                                    }}
                                />
                            </SettingRow>
                            <SettingRow
                                title="Session length"
                                description="The keyboard can't use the microphone, so Simple Voice keeps it ready this long after your last dictation."
                            >
                                <Select
                                    value={String(settings.sessionMinutes)}
                                    options={SESSION_OPTIONS}
                                    onChange={(value) => {
                                        void update({ sessionMinutes: Number(value) });
                                    }}
                                />
                            </SettingRow>
                        </>
                    )}
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
