// FilePath: src/features/settings/transcription/TranscriptionSection.tsx
// Settings → Transcription: the voice model, the languages it is allowed to detect, the optional
// Groq Whisper provider, the live transcript, and the optional model that writes Dictionary names
// into Parakeet dictations. With Smart Select the languages sit inside the Smart Select panel
// (they decide the model); with a chosen model they get their own group.
import { useSettings } from "../../../app/SettingsContext";
import { SettingRow, SettingsGroup, Toggle } from "../../../ui";
import { GroqTranscriptionGroup } from "../groq/GroqTranscriptionGroup";
import { useModels, type ModelsState } from "../models/useModels";
import { ModelList, VoiceModelList } from "../models/VoiceModelList";
import { LanguagePicker } from "./LanguagePicker";
import { SmartSelectPanel, VoiceModelModeSwitch } from "./SmartSelectPanel";
import { useSmartSelect, type SmartSelectState } from "./useSmartSelect";
import { EngineNotice } from "../engine/EngineNotice";
import { useEngineStatus } from "../engine/useEngineStatus";
import "../common.css";
import "../models/models.css";
import "./smartSelect.css";

const LIVE_TITLE = "Live transcript";

/** Streaming is a property of the model that runs, so the note names the model in use. */
function LiveTranscriptRow({ models, smart }: { models: ModelsState; smart: SmartSelectState }) {
    const { settings, update } = useSettings();
    const list = models.models ?? [];
    const activeId = smart.mode === "smart" ? smart.plan?.local : settings.transcriptionModel;
    const active = list.find((m) => m.id === activeId);
    const anyStreams = list.some((m) => m.kind === "transcription" && m.streaming);
    let note: string;
    if (active === undefined) {
        note = "Pick a voice model first.";
    } else if (active.streaming) {
        note = `${active.name} shows words while you talk. The pasted text still comes from the whole recording.`;
    } else if (anyStreams) {
        note = `${active.name} transcribes after you stop. Pick a model marked Live to see words as you speak.`;
    } else {
        note = `${active.name} transcribes after you stop. No voice model here shows words as you speak.`;
    }
    return (
        <SettingRow
            title={LIVE_TITLE}
            description={
                <>
                    Shows the words in the dictation bar and the app while you are still speaking.
                    <p className="sv-inline-note">{note}</p>
                </>
            }
        >
            <Toggle
                label={LIVE_TITLE}
                checked={settings.liveTranscript}
                disabled={active?.streaming !== true}
                onChange={(liveTranscript) => {
                    void update({ liveTranscript });
                }}
            />
        </SettingRow>
    );
}

export function TranscriptionSection() {
    const models = useModels();
    const smart = useSmartSelect(models);
    const engine = useEngineStatus();

    return (
        <>
            <SettingsGroup title="Voice model">
                <div className="sv-smart__mode">
                    <VoiceModelModeSwitch mode={smart.mode} onChange={smart.setMode} />
                </div>
                <EngineNotice engine={engine} />
                {smart.mode === "smart" ? (
                    <SmartSelectPanel
                        models={models}
                        plan={smart.plan}
                        planError={smart.planError}
                    />
                ) : (
                    <>
                        <p className="sv-models-section__intro">
                            Turns your speech into text, entirely on this Mac. A model downloads
                            once; after that dictation works with no network. With Groq on, this
                            model is the fallback and still transcribes meetings.
                        </p>
                        <div className="sv-models-section__flush">
                            <VoiceModelList state={models} />
                        </div>
                    </>
                )}
            </SettingsGroup>

            {smart.mode === "choose" && (
                <SettingsGroup title="Languages">
                    <LanguagePicker />
                </SettingsGroup>
            )}

            <GroqTranscriptionGroup />

            <SettingsGroup title="While you speak">
                <LiveTranscriptRow models={models} smart={smart} />
            </SettingsGroup>

            <SettingsGroup title="Dictionary names">
                <p className="sv-models-section__intro">
                    Names from your Dictionary are always respelled when a voice model writes them
                    close to how you spell them. Parakeet takes no word list, so this small model
                    also checks each Parakeet recording for those names and writes the ones it
                    hears; it adds a fraction of a second after you stop.
                </p>
                <div className="sv-models-section__flush">
                    <ModelList
                        state={models}
                        models={(models.models ?? []).filter((m) => m.kind === "vocabulary")}
                        label="Dictionary names model"
                    />
                </div>
            </SettingsGroup>
        </>
    );
}
