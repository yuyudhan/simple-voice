// FilePath: src/features/settings/transcription/SmartSelectPanel.tsx
// Smart Select (beta) as shown in Settings → Transcription and onboarding: the user picks the
// languages they speak, and the panel lists how a dictation then runs, in order (Groq Whisper
// first while a key is saved, then the one local model with its download state), any language
// the local model can't write as chosen, and the double-check toggle. The mode switch between a
// chosen model and Smart Select lives here too so both screens share it.
import { useId } from "react";
import { ListChecks, Sparkles, TriangleAlert } from "lucide-react";
import type { SmartSelectPlan } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Badge, Segmented, SettingRow, Spinner, Toggle, type SegmentedOption } from "../../../ui";
import { ModelDetails, ModelSide } from "../models/ModelStatus";
import type { ModelsState } from "../models/useModels";
import { ProviderMark } from "../models/VoiceModelList";
import type { VoiceModelMode } from "./useSmartSelect";
import { LanguagePicker } from "./LanguagePicker";
import "../models/models.css";
import "./smartSelect.css";

const MODE_OPTIONS: SegmentedOption<VoiceModelMode>[] = [
    { value: "choose", label: "Choose model", icon: <ListChecks /> },
    {
        value: "smart",
        label: "Smart Select",
        icon: <Sparkles />,
        badge: <Badge tone="neutral">Beta</Badge>,
    },
];

const BETA_NOTE = "Pick the languages you speak. Simple Voice picks the model.";

const RETRY_TITLE = "Double-check unclear dictations";
const RETRY_HINT =
    "Re-run a result that looks wrong on this Mac. Adds about a second when it happens.";

/** When the local model runs: it is the whole route without Groq, the backup with it. */
function localSubtitle(groq: boolean, smartRetry: boolean): string {
    if (!groq) return "On this Mac · audio never leaves it";
    return smartRetry
        ? "On this Mac · when offline or a result looks wrong"
        : "On this Mac · when you're offline";
}

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

function RouteSteps({ plan, models }: { plan: SmartSelectPlan; models: ModelsState }) {
    const { settings } = useSettings();
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
    const model = models.models.find((m) => m.id === plan.local);
    return (
        <ol className="sv-smart__rows">
            {plan.groq && (
                <li className="sv-smart__row">
                    <span className="sv-smart__lead">
                        <span className="sv-smart__step" aria-hidden="true">
                            1
                        </span>
                        <ProviderMark provider="groq" />
                        <span className="sv-model__text">
                            <span className="sv-model__name">Groq Whisper</span>
                            <span className="sv-model__subtitle">Cloud · uses your API key</span>
                        </span>
                    </span>
                    <Badge tone="success">Key saved</Badge>
                </li>
            )}
            <li className="sv-smart__row">
                <span className="sv-smart__lead">
                    <span className="sv-smart__step" aria-hidden="true">
                        {plan.groq ? 2 : 1}
                    </span>
                    {model && <ProviderMark provider={model.provider} />}
                    <span className="sv-model__text">
                        <span className="sv-model__name">{model?.name ?? plan.local}</span>
                        <span className="sv-model__subtitle">
                            {localSubtitle(plan.groq, settings.smartRetry)}
                        </span>
                    </span>
                </span>
                {model && (
                    <>
                        <ModelSide model={model} state={models} deletable={false} />
                        <ModelDetails model={model} state={models} />
                    </>
                )}
            </li>
        </ol>
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
    const routeId = useId();
    return (
        <div className={framed ? "sv-smart sv-smart--framed" : "sv-smart"}>
            <p className="sv-smart__beta">{BETA_NOTE}</p>
            <section className="sv-smart__langs" aria-labelledby={headingId}>
                <h4 id={headingId} className="sv-smart__heading caps-label">
                    Languages you speak
                </h4>
                <LanguagePicker />
            </section>
            <section className="sv-smart__route" aria-labelledby={routeId}>
                <h4 id={routeId} className="sv-smart__heading caps-label">
                    How your dictation runs
                </h4>
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
                        <RouteSteps plan={plan} models={models} />
                        {plan.notice !== null && (
                            <p className="sv-smart__notice" role="note">
                                <TriangleAlert
                                    size={14}
                                    className="sv-smart__notice-icon"
                                    aria-hidden="true"
                                />
                                {plan.notice}
                            </p>
                        )}
                    </>
                )}
            </section>
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
