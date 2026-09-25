// FilePath: src/features/settings/models/VoiceModelList.tsx
// Transcription model picker: one row per model with provider mark, speed/accuracy, languages,
// size, availability and download controls. Selecting a row makes it the active voice model.
import { Apple, Cloud, Download, Trash2 } from "lucide-react";
import type { ModelInfo, ModelProvider } from "../../../lib/api";
import { useSettings } from "../../../app/SettingsContext";
import { Badge, Button, IconButton, ProgressBar, Spinner } from "../../../ui";
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

function StatusBadge({ model, fraction }: { model: ModelInfo; fraction: number | undefined }) {
    if (fraction !== undefined || model.status === "downloading") {
        const value = fraction ?? model.progress ?? 0;
        return <Badge tone="accent">Downloading {Math.round(value * 100)}%</Badge>;
    }
    switch (model.status) {
        case "cloud":
            return <Badge tone="neutral">Cloud</Badge>;
        case "ready":
            return <Badge tone="success">Downloaded</Badge>;
        case "not_downloaded":
            return <Badge tone="neutral">Not downloaded</Badge>;
        case "unsupported":
            return <Badge tone="warning">Unsupported</Badge>;
    }
}

interface RowProps {
    model: ModelInfo;
    active: boolean;
    state: ModelsState;
}

function ModelRow({ model, active, state }: RowProps) {
    const fraction = state.progress[model.id];
    const downloading = fraction !== undefined || model.status === "downloading";
    const unsupported = model.status === "unsupported";
    const pending = state.pendingUse === model.id;
    const failure = state.failures[model.id];
    const size = formatSize(model.sizeMb);
    const local = model.provider !== "groq";

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
            <div className="sv-model__side">
                <StatusBadge model={model} fraction={fraction} />
                {local && model.status === "not_downloaded" && !downloading && (
                    <Button
                        size="sm"
                        variant="secondary"
                        icon={<Download size={14} />}
                        onClick={() => {
                            void state.download(model.id);
                        }}
                    >
                        Download
                    </Button>
                )}
                {local && model.status === "ready" && !active && (
                    <IconButton
                        label={`Delete ${model.name}`}
                        icon={<Trash2 size={15} />}
                        onClick={() => {
                            void state.remove(model.id);
                        }}
                    />
                )}
            </div>
            {downloading && (
                <div className="sv-model__progress">
                    <ProgressBar value={fraction ?? model.progress ?? 0} />
                    {pending && (
                        <span className="sv-model__note">Switches to this model when ready</span>
                    )}
                </div>
            )}
            {unsupported && model.reason && <p className="sv-model__reason">{model.reason}</p>}
            {failure && (
                <p className="sv-model__error" role="alert">
                    {failure}
                </p>
            )}
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
