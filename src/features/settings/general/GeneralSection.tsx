// FilePath: src/features/settings/general/GeneralSection.tsx
import { useCallback, useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "lucide-react";
import { api, errorMessage, type Microphone, type Theme } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import {
    Segmented,
    Select,
    SettingRow,
    SettingsGroup,
    Toggle,
    type SegmentedOption,
} from "../../../ui";
import { AccessibilityWarning } from "../permissions/AccessibilityWarning";
import { ShortcutRecorder } from "../shortcuts/ShortcutRecorder";
import { LanguagePicker } from "./LanguagePicker";
import "../common.css";
import "./general.css";

// Select values are strings; the empty string stands for automatic selection (null).
const AUTOMATIC = "";

const THEME_OPTIONS: SegmentedOption<Theme>[] = [
    { value: "system", label: "System", icon: <Monitor /> },
    { value: "light", label: "Light", icon: <Sun /> },
    { value: "dark", label: "Dark", icon: <Moon /> },
];

const LEARNING_DESCRIPTION =
    "After a dictation is pasted, Simple Voice reads that text field for up to a minute. When " +
    "you fix a misspelled name or term, only the changed words are sent to your post-processing " +
    "model, which decides whether to add them to your dictionary.";

function ThemeSelect() {
    const { settings, update } = useSettings();
    return (
        <Segmented
            label="Theme"
            value={settings.theme}
            options={THEME_OPTIONS}
            onChange={(theme) => {
                void update({ theme });
            }}
        />
    );
}

function MicrophoneSelect() {
    const { settings, update } = useSettings();
    const [microphones, setMicrophones] = useState<Microphone[]>([]);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(
        () =>
            api.listMicrophones().then(
                (next) => {
                    setMicrophones(next);
                    setError(null);
                },
                (e: unknown) => {
                    setError(errorMessage(e));
                },
            ),
        [],
    );

    // Devices come and go (headsets, docks), so the list is re-read whenever the window regains
    // focus instead of only once.
    useEffect(() => {
        void load();
        const onFocus = () => {
            void load();
        };
        window.addEventListener("focus", onFocus);
        return () => {
            window.removeEventListener("focus", onFocus);
        };
    }, [load]);

    const options = [
        { value: AUTOMATIC, label: "Automatic (built-in microphone recommended)" },
        ...microphones.map((m) => ({
            value: m.id,
            label: m.isBuiltIn ? `${m.name} (built-in)` : m.name,
        })),
    ];
    const current = settings.microphone;
    if (current !== null && !microphones.some((m) => m.id === current)) {
        options.push({ value: current, label: `${current} (not connected)` });
    }

    return (
        <div className="sv-general__mic">
            <Select
                value={current ?? AUTOMATIC}
                options={options}
                onChange={(value) => {
                    void update({ microphone: value === AUTOMATIC ? null : value });
                }}
            />
            {error && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

function LearningRow() {
    const { settings, update } = useSettings();
    // The post-processing model is the one deciding what to learn, so without it nothing can be.
    const unavailable = settings.postProcessing === "off";
    return (
        <SettingRow
            title="Learn from your corrections"
            description={
                <>
                    {LEARNING_DESCRIPTION}
                    {unavailable && (
                        <p className="sv-inline-note sv-learning__note">
                            Needs AI post-processing (Settings → Models).
                        </p>
                    )}
                </>
            }
        >
            <Toggle
                label="Learn from your corrections"
                checked={settings.learnFromEdits}
                disabled={unavailable}
                onChange={(learnFromEdits) => {
                    void update({ learnFromEdits });
                }}
            />
        </SettingRow>
    );
}

export function GeneralSection() {
    return (
        <>
            <SettingsGroup title="Appearance">
                <SettingRow title="Theme" description="System follows your Mac's appearance.">
                    <ThemeSelect />
                </SettingRow>
            </SettingsGroup>

            <SettingsGroup title="Shortcuts">
                <SettingRow
                    title="Hold to speak"
                    description="Hold the keys while you talk; dictation stops when you let go."
                >
                    <ShortcutRecorder field="holdShortcut" />
                </SettingRow>
                <SettingRow
                    title="Toggle to speak"
                    description="Press once to start and again to stop. Esc cancels a recording."
                >
                    <ShortcutRecorder field="toggleShortcut" />
                </SettingRow>
                <SettingRow
                    title="Hold to edit"
                    description="Select text in any app, hold the keys and say how to change it, like “make this more formal”. Needs AI post-processing."
                >
                    <ShortcutRecorder field="editShortcut" />
                </SettingRow>
            </SettingsGroup>
            <AccessibilityWarning placement="inline" />

            <SettingsGroup title="Microphone">
                <SettingRow
                    title="Input device"
                    description="Bluetooth headsets drop to a low-quality mode while recording, so the built-in microphone is preferred."
                >
                    <MicrophoneSelect />
                </SettingRow>
            </SettingsGroup>

            <SettingsGroup title="Dictation languages">
                <LanguagePicker />
            </SettingsGroup>

            <SettingsGroup title="Learning">
                <LearningRow />
            </SettingsGroup>
        </>
    );
}
