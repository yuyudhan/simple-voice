// FilePath: src/features/settings/formatting/FormattingProviders.tsx
// Post-processing provider choice: Apple Intelligence, a downloaded local language model, or off.
// Every provider runs on this Mac. The local option lists the downloadable language models with
// their size, source and licence, and a live test formats a fixed sample through the provider.
import { useState, type ReactNode } from "react";
import { Apple, Brain, CircleSlash } from "lucide-react";
import { api, errorMessage, type PostProcessing, type PostProcessingTest } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Button } from "../../../ui";
import type { ModelsState } from "../models/useModels";
import { ModelList } from "../models/VoiceModelList";
import "../common.css";
import "../models/models.css";

interface ProviderOption {
    id: PostProcessing;
    name: string;
    description: string;
    icon: ReactNode;
}

const PROVIDERS: ProviderOption[] = [
    {
        id: "apple",
        name: "Apple Intelligence",
        description: "Built into macOS 26+. The default.",
        icon: <Apple size={16} />,
    },
    {
        id: "local",
        name: "Local model",
        description: "A language model you download once.",
        icon: <Brain size={16} />,
    },
    {
        id: "off",
        name: "Off",
        description: "Rules and capitalisation only.",
        icon: <CircleSlash size={16} />,
    },
];

function TestFormatting({ disabled }: { disabled: boolean }) {
    const [running, setRunning] = useState(false);
    const [result, setResult] = useState<PostProcessingTest | null>(null);
    const [error, setError] = useState<string | null>(null);

    const run = async () => {
        setRunning(true);
        setError(null);
        setResult(null);
        try {
            setResult(await api.testPostProcessing());
        } catch (e) {
            setError(errorMessage(e));
        } finally {
            setRunning(false);
        }
    };

    return (
        <div className="sv-formatting__test">
            <div>
                <Button
                    variant="secondary"
                    size="sm"
                    loading={running}
                    disabled={disabled}
                    onClick={() => {
                        void run();
                    }}
                >
                    Test formatting
                </Button>
            </div>
            {result && (
                <>
                    <pre className="sv-formatting__output">{result.output}</pre>
                    <span className="sv-formatting__latency">
                        Formatted on this Mac in {result.latencyMs} ms
                    </span>
                </>
            )}
            {error && (
                <p className="sv-inline-error" role="alert">
                    {error}
                </p>
            )}
        </div>
    );
}

export function FormattingProviders({ models }: { models: ModelsState }) {
    const { settings, update } = useSettings();
    const list = models.models ?? [];
    const appleIntelligence = list.find((m) => m.id === "apple-intelligence");
    const appleUnavailable = appleIntelligence?.status === "unsupported";
    const localModels = list.filter((m) => m.kind === "post_processing" && m.provider === "llm");
    const localModel = localModels.find((m) => m.id === settings.localLlmModel);
    const localReady = localModel?.status === "ready";

    let providerReady = true;
    if (settings.postProcessing === "apple") providerReady = appleIntelligence?.status === "ready";
    if (settings.postProcessing === "local") providerReady = localReady;

    return (
        <div className="sv-formatting__detail">
            <p className="sv-formatting__note">
                Formatting runs on this Mac. Your text is never sent anywhere.
            </p>
            <div className="sv-providers" role="radiogroup" aria-label="Formatting provider">
                {PROVIDERS.map((provider) => {
                    const disabled =
                        provider.id === "apple" &&
                        appleUnavailable &&
                        settings.postProcessing !== "apple";
                    return (
                        <button
                            key={provider.id}
                            type="button"
                            role="radio"
                            aria-checked={settings.postProcessing === provider.id}
                            className="sv-provider-card"
                            disabled={disabled}
                            title={
                                provider.id === "apple" && appleUnavailable
                                    ? (appleIntelligence.reason ?? undefined)
                                    : undefined
                            }
                            onClick={() => {
                                void update({ postProcessing: provider.id });
                            }}
                        >
                            <span className="sv-provider-card__icon">{provider.icon}</span>
                            <span className="sv-provider-card__name">{provider.name}</span>
                            <span className="sv-provider-card__desc">{provider.description}</span>
                        </button>
                    );
                })}
            </div>

            {settings.postProcessing === "apple" && appleIntelligence && (
                <p className={appleUnavailable ? "sv-inline-warning" : "sv-inline-note"}>
                    {appleUnavailable
                        ? (appleIntelligence.reason ?? "Apple Intelligence is not available.")
                        : "Apple Intelligence is available and runs entirely on this Mac."}
                </p>
            )}

            {settings.postProcessing === "local" && (
                <>
                    {!localReady && (
                        <p className="sv-inline-warning" role="status">
                            {localModel === undefined
                                ? "Pick a local model below. Until one is downloaded, dictations are pasted without formatting."
                                : `${localModel.name} is not downloaded yet. Until it is, dictations are pasted without formatting.`}
                        </p>
                    )}
                    <ModelList
                        state={models}
                        models={localModels}
                        label="Local language model"
                        activeId={settings.localLlmModel}
                    />
                </>
            )}

            {settings.postProcessing === "off" ? (
                <p className="sv-inline-note">
                    Only your dictionary rules, snippets, brand casing, spacing and capitalisation
                    are applied.
                </p>
            ) : (
                <TestFormatting disabled={!providerReady} />
            )}
        </div>
    );
}
