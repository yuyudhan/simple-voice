// FilePath: src/features/settings/dictation/DictationSection.tsx
// Settings → Dictation: everything between pressing a shortcut and the text landing in the app:
// how a recording starts, what it records, what context it may read, how the text is pasted,
// and the cues along the way.
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, type Microphone } from "../../../lib/api";
import { isLinux } from "../../../lib/platform";
import { useSettings } from "../../../app/SettingsContext";
import { Select, SettingRow, SettingsGroup, Toggle } from "../../../ui";
import { AccessibilityWarning } from "../permissions/AccessibilityWarning";
import { ShortcutRecorder } from "../shortcuts/ShortcutRecorder";
import { ContextGroup } from "./ContextGroup";
import { FeedbackGroup } from "./FeedbackGroup";
import { EscapeCancelsRow } from "./EscapeCancelsRow";
import { BUILT_IN_OR_DEFAULT, microphoneMenu } from "./microphoneMenu";
import "../common.css";
import "./dictation.css";

const MAX_LENGTH_MINUTES = [1, 2, 5, 10, 15, 30];

function MicrophoneSelect() {
    const { settings, update } = useSettings();
    const [microphones, setMicrophones] = useState<Microphone[] | null>(null);
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

    const menu = microphoneMenu(microphones, settings.microphone, isLinux);

    return (
        <div className="sv-dictation__mic">
            <Select
                value={menu.value}
                options={menu.options}
                onChange={(value) => {
                    void update({ microphone: value === BUILT_IN_OR_DEFAULT ? null : value });
                }}
            />
            {menu.note && <p className="sv-dictation__mic-note">{menu.note}</p>}
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
                description={
                    isLinux
                        ? "Simple Voice records from the input chosen in your desktop's sound settings."
                        : "Simple Voice records from the built-in microphone unless you choose another device. Bluetooth headsets drop to a low-quality mode while recording, so connecting one never switches the input."
                }
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
                title="Mute speakers while recording"
                description="Every app and browser tab goes silent while you dictate, so the microphone hears only you. When you stop, sound comes back exactly as it was; speakers you had muted stay muted."
            >
                <Toggle
                    label="Mute speakers while recording"
                    checked={settings.muteWhileRecording}
                    onChange={(muteWhileRecording) => {
                        void update({ muteWhileRecording });
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
            <AccessibilityWarning />

            <RecordingGroup />

            <ContextGroup />

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
