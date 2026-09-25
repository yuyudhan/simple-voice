// FilePath: src/features/settings/system/SystemSection.tsx
import { useState } from "react";
import { Play, Volume1, Volume2 } from "lucide-react";
import { api, errorMessage, type SoundTheme } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { IconButton, Select, SettingRow, SettingsGroup, Toggle, useToast } from "../../../ui";
import "./system.css";

const THEMES: { id: SoundTheme; name: string; description: string }[] = [
    { id: "soft", name: "Soft", description: "Warm, low tones" },
    { id: "glass", name: "Glass", description: "Bright and crisp" },
    { id: "pop", name: "Pop", description: "Short and playful" },
    { id: "chime", name: "Chime", description: "A two-note bell" },
];

const MAX_LENGTH_MINUTES = [1, 2, 5, 10, 15, 30];

function SoundThemePicker() {
    const { settings, update } = useSettings();
    const { toast } = useToast();
    const disabled = !settings.sounds;

    return (
        <div
            className={disabled ? "sv-themes is-disabled" : "sv-themes"}
            role="radiogroup"
            aria-label="Sound theme"
        >
            {THEMES.map((theme) => {
                const selected = settings.soundTheme === theme.id;
                return (
                    <div key={theme.id} className={selected ? "sv-theme is-selected" : "sv-theme"}>
                        <button
                            type="button"
                            role="radio"
                            aria-checked={selected}
                            className="sv-theme__pick"
                            disabled={disabled}
                            onClick={() => {
                                void update({ soundTheme: theme.id });
                            }}
                        >
                            <span className="sv-theme__name">{theme.name}</span>
                            <span className="sv-theme__desc">{theme.description}</span>
                        </button>
                        <IconButton
                            label={`Play ${theme.name}`}
                            icon={<Play size={13} />}
                            onClick={() => {
                                void api.previewSound(theme.id).catch((e: unknown) => {
                                    toast(errorMessage(e), "danger");
                                });
                            }}
                        />
                    </div>
                );
            })}
        </div>
    );
}

function VolumeSliderInner({ value, disabled }: { value: number; disabled: boolean }) {
    const { update } = useSettings();
    const [draft, setDraft] = useState(Math.round(value * 100));
    const commit = () => {
        if (draft !== Math.round(value * 100)) void update({ soundVolume: draft / 100 });
    };
    return (
        <div className="sv-volume">
            <Volume1 size={15} aria-hidden="true" />
            <input
                type="range"
                min={0}
                max={100}
                step={1}
                value={draft}
                disabled={disabled}
                aria-label="Sound volume"
                aria-valuetext={`${String(draft)}%`}
                className="sv-volume__range"
                onChange={(event) => {
                    setDraft(Number(event.target.value));
                }}
                onPointerUp={commit}
                onKeyUp={commit}
                onBlur={commit}
            />
            <Volume2 size={15} aria-hidden="true" />
            <span className="sv-volume__value">{draft}%</span>
        </div>
    );
}

/** Remounts on external changes so the draft follows the stored value between drags. */
function VolumeSlider({ value, disabled }: { value: number; disabled: boolean }) {
    return <VolumeSliderInner key={value} value={value} disabled={disabled} />;
}

export function SystemSection() {
    const { settings, update } = useSettings();

    const maxOptions = MAX_LENGTH_MINUTES.map((m) => ({
        value: String(m * 60),
        label: m === 1 ? "1 minute" : `${String(m)} minutes`,
    }));
    if (!maxOptions.some((o) => o.value === String(settings.maxRecordingSeconds))) {
        maxOptions.push({
            value: String(settings.maxRecordingSeconds),
            label: `${String(Math.round(settings.maxRecordingSeconds / 60))} minutes`,
        });
    }

    return (
        <>
            <SettingsGroup title="App settings">
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
                    title="Show floating bar at all times"
                    description="Keep the dictation pill on screen even when you are not speaking."
                >
                    <Toggle
                        label="Show floating bar at all times"
                        checked={settings.showBarAlways}
                        onChange={(showBarAlways) => {
                            void update({ showBarAlways });
                        }}
                    />
                </SettingRow>
                <SettingRow
                    title="Show app in Dock"
                    description="When off, Simple Voice lives only in the menu bar."
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

            <SettingsGroup title="Sound">
                <SettingRow
                    title="Dictation sounds"
                    description="Play a short cue when dictation starts and stops."
                >
                    <Toggle
                        label="Dictation sounds"
                        checked={settings.sounds}
                        onChange={(sounds) => {
                            void update({ sounds });
                        }}
                    />
                </SettingRow>
                <SoundThemePicker />
                <SettingRow title="Volume" description="How loud the cues play.">
                    <VolumeSlider value={settings.soundVolume} disabled={!settings.sounds} />
                </SettingRow>
                <SettingRow
                    title="Mute all audio while dictating"
                    description="Silences music and videos while you speak so the microphone hears only you. Your previous output state is restored afterwards."
                >
                    <Toggle
                        label="Mute all audio while dictating"
                        checked={settings.muteWhileDictating}
                        onChange={(muteWhileDictating) => {
                            void update({ muteWhileDictating });
                        }}
                    />
                </SettingRow>
            </SettingsGroup>

            <SettingsGroup title="Behaviour">
                <SettingRow
                    title="Restore clipboard after pasting"
                    description="Text is pasted through the clipboard; put back what was there before."
                >
                    <Toggle
                        label="Restore clipboard after pasting"
                        checked={settings.restoreClipboard}
                        onChange={(restoreClipboard) => {
                            void update({ restoreClipboard });
                        }}
                    />
                </SettingRow>
                <SettingRow
                    title="Maximum recording length"
                    description="A recording that runs this long is stopped and transcribed automatically."
                >
                    <Select
                        value={String(settings.maxRecordingSeconds)}
                        options={maxOptions}
                        onChange={(value) => {
                            void update({ maxRecordingSeconds: Number(value) });
                        }}
                    />
                </SettingRow>
            </SettingsGroup>
        </>
    );
}
