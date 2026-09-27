// FilePath: src/features/settings/models/VoiceModelList.tsx
// Transcription model picker: one row per model with provider mark, speed/accuracy, languages,
// size, availability and download controls. Selecting a row makes it the active voice model.
import { Apple, AudioWaveform, Cloud } from "lucide-react";
import type { ModelInfo, ModelProvider } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Spinner } from "../../../ui";
import { ModelDetails, ModelSide } from "./ModelStatus";
import type { ModelsState } from "./useModels";
import "./models.css";

export function ProviderMark({ provider }: { provider: ModelProvider }) {
    if (provider === "parakeet") {
        return (
            <span className="sv-provider sv-provider--nvidia" aria-label="NVIDIA Parakeet">
                N
            </span>
        );
    }
    if (provider === "apple") {
        return (
            <span className="sv-provider sv-provider--apple" aria-label="Apple">
                <Apple size={15} strokeWidth={1.8} />
            </span>
        );
    }
    if (provider === "whisper") {
        return (
            <span className="sv-provider sv-provider--whisper" aria-label="Whisper on this Mac">
                <AudioWaveform size={15} strokeWidth={1.8} />
            </span>
        );
    }
    return (
        <span className="sv-provider sv-provider--groq" aria-label="Groq cloud">
            <Cloud size={15} strokeWidth={1.8} />
        </span>
    );
}

function formatSize(sizeMb: number | null): string | null {
    if (sizeMb === null) return null;
    return sizeMb >= 1000 ? `${(sizeMb / 1000).toFixed(1)} GB` : `${String(sizeMb)} MB`;
}

interface RowProps {
    model: ModelInfo;
    active: boolean;
    state: ModelsState;
}

function ModelRow({ model, active, state }: RowProps) {
    const unsupported = model.status === "unsupported";
    const size = formatSize(model.sizeMb);
    return (
        <div
            className={[
                "sv-model",
                active ? "is-active" : "",
                unsupported ? "is-unsupported" : "",
            ].join(" ")}
        >
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
                <span className="sv-model__text">
                    <span className="sv-model__name">{model.name}</span>
                    <span className="sv-model__subtitle">{model.subtitle}</span>
                    <span className="sv-model__meta">
                        <span className="sv-model__stat">
                            <span className="sv-model__stat-label">Speed</span>
                            {model.speed}%
                        </span>
                        <span className="sv-model__stat">
                            <span className="sv-model__stat-label">Accuracy</span>
                            {model.accuracy}%
                        </span>
                        <span>{model.languages}</span>
                        {size && <span>{size}</span>}
                    </span>
                </span>
            </button>
            <ModelSide model={model} state={state} deletable={!active} />
            <ModelDetails model={model} state={state} />
        </div>
    );
}

export function VoiceModelList({ state }: { state: ModelsState }) {
    const { settings } = useSettings();

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

    const voiceModels = state.models.filter((m) => m.kind === "transcription");
    return (
        <div className="sv-models" role="radiogroup" aria-label="Voice model">
            {voiceModels.map((model) => (
                <ModelRow
                    key={model.id}
                    model={model}
                    active={settings.transcriptionModel === model.id}
                    state={state}
                />
            ))}
        </div>
    );
}
