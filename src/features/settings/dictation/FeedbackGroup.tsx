// FilePath: src/features/settings/dictation/FeedbackGroup.tsx
// Dictation → Feedback: the floating bar and the start/stop cues, both of which tell the user a
// recording is live.
import { useState } from "react";
import { Play, Volume1, Volume2 } from "lucide-react";
import { api, errorMessage, type SoundTheme } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { IconButton, Select, SettingRow, SettingsGroup, Toggle, useToast } from "../../../ui";
import "./feedback.css";

const THEMES: { id: SoundTheme; name: string; description: string }[] = [
    { id: "soft", name: "Soft", description: "Warm, low tones" },
    { id: "glass", name: "Glass", description: "Bright and crisp" },
    { id: "pop", name: "Pop", description: "Short and playful" },
    { id: "chime", name: "Chime", description: "A two-note bell" },
];

const PASTED_TEXT_SECONDS = [1, 2, 3, 5, 8];

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

export function FeedbackGroup() {
    const { settings, update } = useSettings();

    const pastedTextOptions = PASTED_TEXT_SECONDS.map((s) => ({
        value: String(s),
        label: s === 1 ? "1 second" : `${String(s)} seconds`,
    }));
    if (!PASTED_TEXT_SECONDS.includes(settings.pastedTextSeconds)) {
        pastedTextOptions.push({
            value: String(settings.pastedTextSeconds),
            label: `${String(settings.pastedTextSeconds)} seconds`,
        });
    }

    return (
        <SettingsGroup title="Feedback">
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
                title="Show pasted text for"
                description="How long the pill shows the start of what was pasted after a dictation."
            >
                <Select
                    value={String(settings.pastedTextSeconds)}
                    options={pastedTextOptions}
                    onChange={(value) => {
                        void update({ pastedTextSeconds: Number(value) });
                    }}
                />
            </SettingRow>
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
        </SettingsGroup>
    );
}
