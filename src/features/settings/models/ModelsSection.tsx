// FilePath: src/features/settings/models/ModelsSection.tsx
import { useSettings } from "../../../app/SettingsContext";
import { SettingsGroup } from "../../../ui";
import { FormattingProviders } from "./FormattingProviders";
import { GroqKeyField } from "./GroqKeyField";
import { useModels } from "./useModels";
import { VoiceModelList } from "./VoiceModelList";
import "./models.css";

export function ModelsSection() {
    const { settings } = useSettings();
    const models = useModels();
    // One key serves both uses; it is shown next to whichever Groq feature is active first.
    const groqVoice = settings.transcriptionModel === "groq-whisper";
    const groqFormatting = !groqVoice && settings.postProcessing === "groq";
    const appleIntelligence = models.models?.find((m) => m.id === "apple-intelligence");

    return (
        <>
            <SettingsGroup title="Voice model">
                <p className="sv-models-section__intro">
                    Turns your speech into text. Cloud models need a Groq key; local models run
                    entirely on this Mac once downloaded.
                </p>
                <div className="sv-models-section__flush">
                    <VoiceModelList state={models} />
                </div>
                {groqVoice && (
                    <div className="sv-models-section__key">
                        <GroqKeyField />
                    </div>
                )}
            </SettingsGroup>

            <SettingsGroup title="Formatting">
                <div className="sv-models-section">
                    <FormattingProviders appleIntelligence={appleIntelligence} />
                </div>
                {groqFormatting && (
                    <div className="sv-models-section__key">
                        <GroqKeyField />
                    </div>
                )}
            </SettingsGroup>
        </>
    );
}
