// FilePath: src/features/settings/transcription/TranscriptionSection.tsx
// Settings → Transcription: the voice model and the languages it is allowed to detect. With
// Smart Select the languages sit inside the Smart Select panel (they decide the model); with a
// chosen model they get their own group.
import { SettingsGroup } from "../../../ui";
import { GroqKeyField } from "../models/GroqKeyField";
import { useModels } from "../models/useModels";
import { VoiceModelList } from "../models/VoiceModelList";
import { LanguagePicker } from "./LanguagePicker";
import { SmartSelectPanel, VoiceModelModeSwitch } from "./SmartSelectPanel";
import { useSmartSelect } from "./useSmartSelect";
import "../models/models.css";
import "./smartSelect.css";

export function TranscriptionSection() {
    const models = useModels();
    const smart = useSmartSelect(models);

    return (
        <>
            <SettingsGroup title="Voice model">
                <div className="sv-smart__mode">
                    <VoiceModelModeSwitch mode={smart.mode} onChange={smart.setMode} />
                </div>
                {smart.mode === "smart" ? (
                    <SmartSelectPanel
                        models={models}
                        plan={smart.plan}
                        planError={smart.planError}
                    />
                ) : (
                    <>
                        <p className="sv-models-section__intro">
                            Turns your speech into text. Cloud models need a Groq key; local models
                            run entirely on this Mac once downloaded.
                        </p>
                        <div className="sv-models-section__flush">
                            <VoiceModelList state={models} />
                        </div>
                    </>
                )}
                <div className="sv-models-section__key">
                    <GroqKeyField />
                </div>
            </SettingsGroup>

            {smart.mode === "choose" && (
                <SettingsGroup title="Languages">
                    <LanguagePicker />
                </SettingsGroup>
            )}
        </>
    );
}
