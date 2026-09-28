// FilePath: src/features/settings/dictation/DictationSection.tsx
// Settings → Dictation: everything between pressing a shortcut and the text landing in the app:
// how a recording starts, what it records, how the text is pasted, and the cues along the way.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type Microphone } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Select, SettingRow, SettingsGroup, Toggle } from "../../../ui";
import { AccessibilityWarning } from "../permissions/AccessibilityWarning";
import { ShortcutRecorder } from "../shortcuts/ShortcutRecorder";
import { FeedbackGroup } from "./FeedbackGroup";
import { EscapeCancelsRow } from "./EscapeCancelsRow";
import "../common.css";
import "./dictation.css";

// Select values are strings; the empty string stands for automatic selection (null).
const AUTOMATIC = "";

const MAX_LENGTH_MINUTES = [1, 2, 5, 10, 15, 30];

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
        <div className="sv-dictation__mic">
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

function RecordingGroup() {
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
        <SettingsGroup title="Recording">
            <SettingRow
                title="Input device"
                description="Bluetooth headsets drop to a low-quality mode while recording, so the built-in microphone is preferred."
            >
                <MicrophoneSelect />
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
    );
}

export function DictationSection() {
    const { settings, update } = useSettings();

    return (
        <>
            <SettingsGroup title="Shortcuts">
                <SettingRow
                    title="Hold to speak"
                    description="Hold the keys while you talk; dictation stops when you let go."
                >
                    <ShortcutRecorder field="holdShortcut" />
                </SettingRow>
                <SettingRow
                    title="Toggle to speak"
                    description="Press once to start and again to stop."
                >
                    <ShortcutRecorder field="toggleShortcut" />
                </SettingRow>
                <SettingRow
                    title="Toggle to edit"
                    description="Select text in any app, press the keys, say how to change it, like “make this more formal”, and press again to apply. Needs AI post-processing."
                >
                    <ShortcutRecorder field="editShortcut" />
                </SettingRow>
                <EscapeCancelsRow />
            </SettingsGroup>
            <AccessibilityWarning placement="inline" />

            <RecordingGroup />

            <SettingsGroup title="Pasting">
                <SettingRow
                    title="Restore clipboard after pasting"
                    description="Text is pasted through the clipboard. Off keeps the dictated text there; on puts back what was there before."
                >
                    <Toggle
                        label="Restore clipboard after pasting"
                        checked={settings.restoreClipboard}
                        onChange={(restoreClipboard) => {
                            void update({ restoreClipboard });
                        }}
                    />
                </SettingRow>
            </SettingsGroup>

            <FeedbackGroup />
        </>
    );
}
