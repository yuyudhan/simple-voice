// FilePath: src/features/settings/formatting/FormattingSection.tsx
// Settings → Formatting: the post-processing provider and what it may learn. Learning lives here
// because the provider is what decides which corrections are worth keeping.
import { useSettings } from "../../../app/SettingsContext";
import { SettingRow, SettingsGroup, Toggle } from "../../../ui";
import { GroqKeyField } from "../models/GroqKeyField";
import { useModels } from "../models/useModels";
import { FormattingProviders } from "./FormattingProviders";
import "../common.css";
import "../models/models.css";
import "./formatting.css";

const LEARNING_DESCRIPTION =
    "After a dictation is pasted, Simple Voice reads that text field for up to a minute. When " +
    "you fix a misspelled name or term, only the changed words are sent to your formatting " +
    "provider, which decides whether to add them to your dictionary.";

function LearningRow() {
    const { settings, update } = useSettings();
    // The provider is the one deciding what to learn, so without it nothing can be.
    const unavailable = settings.postProcessing === "off";
    return (
        <SettingRow
            title="Learn from your corrections"
            description={
                <>
                    {LEARNING_DESCRIPTION}
                    {unavailable && (
                        <p className="sv-inline-note sv-learning__note">
                            Needs a formatting provider above.
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

export function FormattingSection() {
    const { settings } = useSettings();
    const models = useModels();
    const appleIntelligence = models.models?.find((m) => m.id === "apple-intelligence");

    return (
        <>
            <SettingsGroup title="Provider">
                <div className="sv-models-section">
                    <FormattingProviders appleIntelligence={appleIntelligence} />
                </div>
                {settings.postProcessing === "groq" && (
                    <div className="sv-models-section__key">
                        <GroqKeyField />
                    </div>
                )}
            </SettingsGroup>

            <SettingsGroup title="Learning">
                <LearningRow />
            </SettingsGroup>
        </>
    );
}
