// FilePath: src/features/settings/transcription/SmartSelectPanel.tsx
// Smart Select as shown in Settings → Transcription and onboarding: the user picks the languages
// they speak, and the panel shows the model Smart Select runs for them with its download state,
// whether Groq runs first, and the double-check toggle. The mode switch between Smart Select and
// a chosen model lives here too so both screens share it.
import { useId } from "react";
import { ListChecks, Sparkles } from "lucide-react";
import type { SmartSelectPlan } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Segmented, SettingRow, Spinner, Toggle, type SegmentedOption } from "../../../ui";
import { ModelDetails, ModelSide } from "../models/ModelStatus";
import type { ModelsState } from "../models/useModels";
import { ProviderMark } from "../models/VoiceModelList";
import type { VoiceModelMode } from "./useSmartSelect";
import { LanguagePicker } from "./LanguagePicker";
import "../models/models.css";
import "./smartSelect.css";

const MODE_OPTIONS: SegmentedOption<VoiceModelMode>[] = [
    { value: "smart", label: "Smart Select", icon: <Sparkles /> },
    { value: "choose", label: "Choose model", icon: <ListChecks /> },
];

const RETRY_TITLE = "Double-check unclear dictations";
const RETRY_HINT =
    "If a result looks wrong, try the other model once. Adds about a second when it happens.";
const GROQ_FIRST = "Groq Whisper runs first while your API key is saved;";
const LOCAL_ONLY = "Runs on this Mac. Audio never leaves it.";

export function VoiceModelModeSwitch({
    mode,
    onChange,
}: {
    mode: VoiceModelMode;
    onChange: (mode: VoiceModelMode) => void;
}) {
    return (
        <Segmented label="Voice model" value={mode} options={MODE_OPTIONS} onChange={onChange} />
    );
}

function PlanRows({ plan, models }: { plan: SmartSelectPlan; models: ModelsState }) {
    if (models.models === null) {
        return (
            <div className="sv-models__loading">
                {models.loadError ? (
                    <p className="sv-model__error" role="alert">
                        {models.loadError}
                    </p>
                ) : (
                    <Spinner />
                )}
            </div>
        );
    }
    const catalog = models.models;
    return (
        <ul className="sv-smart__rows" aria-label="Model Smart Select uses">
            {plan.rows.map((row) => {
                const model = catalog.find((m) => m.id === row.model);
                return (
                    <li key={row.model} className="sv-smart__row">
                        <span className="sv-smart__lead">
                            {model && <ProviderMark provider={model.provider} />}
                            <span className="sv-model__text">
                                <span className="sv-model__name">{model?.name ?? row.model}</span>
                                <span className="sv-model__subtitle">{row.purpose}</span>
                            </span>
                        </span>
                        {model && (
                            <>
                                <ModelSide model={model} state={models} deletable={false} />
                                <ModelDetails model={model} state={models} />
                            </>
                        )}
                    </li>
                );
            })}
        </ul>
    );
}

interface PanelProps {
    models: ModelsState;
    plan: SmartSelectPlan | null;
    planError: string | null;
    /** Draw its own hairline frame (onboarding); inside a settings group the group frames it. */
    framed?: boolean;
}

export function SmartSelectPanel({ models, plan, planError, framed = false }: PanelProps) {
    const { settings, update } = useSettings();
    const headingId = useId();
    const local = plan?.rows[0];
    const localName =
        models.models?.find((m) => m.id === local?.model)?.name ??
        local?.model ??
        "the local model";
    return (
        <div className={framed ? "sv-smart sv-smart--framed" : "sv-smart"}>
            <section className="sv-smart__langs" aria-labelledby={headingId}>
                <h4 id={headingId} className="sv-smart__heading caps-label">
                    Languages you speak
                </h4>
                <LanguagePicker />
            </section>
            {plan === null ? (
                <div className="sv-models__loading">
                    {planError ? (
                        <p className="sv-model__error" role="alert">
                            {planError}
                        </p>
                    ) : (
                        <Spinner />
                    )}
                </div>
            ) : (
                <>
                    <PlanRows plan={plan} models={models} />
                    <p className="sv-smart__intro">
                        {plan.groq
                            ? `${GROQ_FIRST} ${localName} runs when you're offline or a Groq ` +
                              "result looks unclear."
                            : LOCAL_ONLY}
                    </p>
                </>
            )}
            {/* Without a Groq key the route has one local model, so there is nothing to retry. */}
            {plan?.groq && (
                <SettingRow title={RETRY_TITLE} description={RETRY_HINT}>
                    <Toggle
                        label={RETRY_TITLE}
                        checked={settings.smartRetry}
                        onChange={(smartRetry) => {
                            void update({ smartRetry });
                        }}
                    />
                </SettingRow>
            )}
        </div>
    );
}
