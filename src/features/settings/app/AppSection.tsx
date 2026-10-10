// FilePath: src/features/settings/app/AppSection.tsx
// Settings → App: how Simple Voice looks, starts and stays up to date.
import { Monitor, Moon, Sun } from "lucide-react";
import type { Theme } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Segmented, SettingRow, SettingsGroup, Toggle, type SegmentedOption } from "../../../ui";
import { UpdatesGroup } from "../../updates/UpdatesGroup";

const THEME_OPTIONS: SegmentedOption<Theme>[] = [
    { value: "system", label: "System", icon: <Monitor /> },
    { value: "light", label: "Light", icon: <Sun /> },
    { value: "dark", label: "Dark", icon: <Moon /> },
];

export function AppSection() {
    const { settings, update } = useSettings();

    return (
        <>
            <SettingsGroup title="Appearance">
                <SettingRow title="Theme" description="System follows your Mac's appearance.">
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

            <SettingsGroup title="Startup">
                <SettingRow
                    title="Launch at login"
                    description="Start Simple Voice when you log in so the shortcuts always work."
                >
                    <Toggle
                        label="Launch at login"
                        checked={settings.launchAtLogin}
                        onChange={(launchAtLogin) => {
                            void update({ launchAtLogin });
                        }}
                    />
                </SettingRow>
                <SettingRow
                    title="Show app in Dock"
                    description="Shows the Dock icon while the window is open. When off, Simple Voice lives only in the menu bar."
                >
                    <Toggle
                        label="Show app in Dock"
                        checked={settings.showInDock}
                        onChange={(showInDock) => {
                            void update({ showInDock });
                        }}
                    />
                </SettingRow>
            </SettingsGroup>

            <UpdatesGroup />
        </>
    );
}
