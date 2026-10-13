// FilePath: src/features/settings/models/VoiceModelList.tsx
// Transcription model picker: one row per model with provider mark, speed/accuracy, languages,
// size, licence, download source, availability and download controls. Selecting a row makes it
// the active voice model. Every model runs on this Mac; only the one-time download uses the
// network.
import { Apple, AudioWaveform, Brain, Languages, Users } from "lucide-react";
import type { ModelInfo, ModelProvider } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Badge, Spinner } from "../../../ui";
import { ModelDetails, ModelSide } from "./ModelStatus";
import type { ModelsState } from "./useModels";
import "./models.css";

export function ProviderMark({ provider }: { provider: ModelProvider }) {
    switch (provider) {
        case "parakeet":
            return (
                <span className="sv-provider sv-provider--nvidia" aria-label="NVIDIA Parakeet">
                    N
                </span>
            );
        case "apple":
            return (
                <span className="sv-provider sv-provider--apple" aria-label="Apple">
                    <Apple size={15} strokeWidth={1.8} />
                </span>
            );
        case "whisper":
            return (
                <span className="sv-provider sv-provider--whisper" aria-label="Whisper on this Mac">
                    <AudioWaveform size={15} strokeWidth={1.8} />
                </span>
            );
        case "llm":
            return (
                <span className="sv-provider sv-provider--llm" aria-label="Local language model">
                    <Brain size={15} strokeWidth={1.8} />
                </span>
            );
        case "diarizer":
            return (
                <span className="sv-provider sv-provider--diarizer" aria-label="Speaker model">
                    <Users size={15} strokeWidth={1.8} />
                </span>
            );
        case "translator":
            return (
                <span
                    className="sv-provider sv-provider--translator"
                    aria-label="Translation model"
                >
                    <Languages size={15} strokeWidth={1.8} />
                </span>
            );
    }
}

export function formatSize(sizeMb: number | null): string | null {
    if (sizeMb === null) return null;
    return sizeMb >= 1000 ? `${(sizeMb / 1000).toFixed(1)} GB` : `${String(sizeMb)} MB`;
}

/** Host of a download URL, so the row names where the weights come from. */
function sourceHost(source: string): string {
    try {
        return new URL(source).host;
    } catch {
        return source;
    }
}

/** Download size, origin and licence of a model; nothing for models built into macOS. */
export function ModelProvenance({ model }: { model: ModelInfo }) {
    const size = formatSize(model.sizeMb);
    if (model.source === null && model.license === null) {
        return <span className="sv-model__provenance">Built into macOS · no download</span>;
    }
    return (
        <span className="sv-model__provenance">
            {size && <span>One-time download {size}</span>}
            {model.source !== null && (
                <span title={model.source}>from {sourceHost(model.source)}</span>
            )}
            {model.license !== null && <span>Licence {model.license}</span>}
        </span>
    );
}

interface RowProps {
    model: ModelInfo;
    /** Pickable rows are radios; a model without a choice (speaker model) only downloads. */
    pickable: boolean;
    active: boolean;
    state: ModelsState;
}

function ModelRow({ model, pickable, active, state }: RowProps) {
    const unsupported = model.status === "unsupported";
    const text = (
        <span className="sv-model__text">
            <span className="sv-model__name">
                {model.name}
                {model.streaming && (
                    <Badge tone="accent" title="Shows words while you are still speaking">
                        Live
                    </Badge>
                )}
            </span>
            <span className="sv-model__subtitle">{model.subtitle}</span>
            <span className="sv-model__meta">
                {model.kind === "transcription" && (
                    <>
                        <span className="sv-model__stat">
                            <span className="sv-model__stat-label">Speed</span>
                            {model.speed}%
                        </span>
                        <span className="sv-model__stat">
                            <span className="sv-model__stat-label">Accuracy</span>
                            {model.accuracy}%
                        </span>
                    </>
                )}
                <span>{model.languages}</span>
            </span>
            <ModelProvenance model={model} />
        </span>
    );
    return (
        <div
            className={[
                "sv-model",
                active ? "is-active" : "",
                unsupported ? "is-unsupported" : "",
            ].join(" ")}
            role={pickable ? undefined : "listitem"}
        >
            {pickable ? (
                <button
                    type="button"
                    role="radio"
                    aria-checked={active}
                    className="sv-model__pick"
                    disabled={unsupported}
                    onClick={() => {
                        void state.use(model);
                    }}
                >
                    <span className="sv-model__radio" aria-hidden="true" />
                    <ProviderMark provider={model.provider} />
                    {text}
                </button>
            ) : (
                <div className="sv-model__pick sv-model__pick--static">
                    <ProviderMark provider={model.provider} />
                    {text}
                </div>
            )}
            <ModelSide model={model} state={state} deletable={!active} />
            <ModelDetails model={model} state={state} />
        </div>
    );
}

interface ListProps {
    state: ModelsState;
    models: ModelInfo[];
    label: string;
    /** Id of the model in use; undefined when the list offers no choice. */
    activeId?: string;
}

/** Rows of local models with their download state; a radio group when `activeId` is given. */
export function ModelList({ state, models, label, activeId }: ListProps) {
    if (state.models === null) {
        return (
            <div className="sv-models__loading">
                {state.loadError ? (
                    <p className="sv-model__error" role="alert">
                        {state.loadError}
                    </p>
                ) : (
                    <Spinner />
                )}
            </div>
        );
    }
    const pickable = activeId !== undefined;
    return (
        <div className="sv-models" role={pickable ? "radiogroup" : "list"} aria-label={label}>
            {models.map((model) => (
                <ModelRow
                    key={model.id}
                    model={model}
                    pickable={pickable}
                    active={activeId === model.id}
                    state={state}
                />
            ))}
        </div>
    );
}

export function VoiceModelList({ state }: { state: ModelsState }) {
    const { settings } = useSettings();
    return (
        <ModelList
            state={state}
            models={(state.models ?? []).filter((m) => m.kind === "transcription")}
            label="Voice model"
            activeId={settings.transcriptionModel}
        />
    );
}
