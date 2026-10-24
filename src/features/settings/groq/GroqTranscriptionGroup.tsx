// FilePath: src/features/settings/groq/GroqTranscriptionGroup.tsx
// Settings → Transcription: the opt-in switch to Groq Whisper, its model, the key, and what
// leaves the Mac. Off by default; the on-device voice model stays chosen as the fallback and
// still transcribes meetings.
import { useSettings } from "../../../app/SettingsContext";
import { Select, SettingRow, SettingsGroup, Toggle, type SelectOption } from "../../../ui";
import { CloudNotice, GroqKeyField } from "./GroqKeyField";
import { useGroqKey } from "./useGroqKey";
import "./groq.css";

const TITLE = "Transcribe with Groq";

const WHISPER_MODELS: SelectOption<string>[] = [
    { value: "whisper-large-v3-turbo", label: "Whisper Large v3 Turbo (faster)" },
    { value: "whisper-large-v3", label: "Whisper Large v3 (most accurate)" },
];

export function GroqTranscriptionGroup() {
    const { settings, update } = useSettings();
    const groq = useGroqKey();
    const on = settings.transcriptionProvider === "groq";

    return (
        <SettingsGroup title="Cloud transcription (optional)">
            <SettingRow
                title={TITLE}
                description={
                    <>
                        Sends each dictation's recording to Groq Whisper with your own API key
                        instead of transcribing it on this Mac. The voice model above takes over
                        when Groq fails or is slow, and always transcribes meetings.
                        {!on && groq.saved === false && (
                            <p className="sv-inline-note">Save a Groq API key below first.</p>
                        )}
                    </>
                }
            >
                <Toggle
                    label={TITLE}
                    checked={on}
                    disabled={!on && groq.saved !== true}
                    onChange={(enabled) => {
                        void update({ transcriptionProvider: enabled ? "groq" : "local" });
                    }}
                />
            </SettingRow>
            {on && (
                <SettingRow title="Groq model" description="Both understand the same languages.">
                    <Select
                        ariaLabel="Groq Whisper model"
                        value={settings.groqWhisperModel}
                        options={WHISPER_MODELS}
                        onChange={(groqWhisperModel) => {
                            void update({ groqWhisperModel });
                        }}
                    />
                </SettingRow>
            )}
            <div className="sv-groq-panel">
                {on && (
                    <CloudNotice>
                        Audio leaves this Mac. Each dictation's recording, with your dictionary
                        words as spelling hints, is uploaded to Groq (api.groq.com) under your
                        account. While Groq is on, text around the cursor is never read for context.
                        Turn this off to keep everything on this Mac.
                    </CloudNotice>
                )}
                <GroqKeyField groq={groq} />
            </div>
        </SettingsGroup>
    );
}
