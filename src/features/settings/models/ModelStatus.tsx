// FilePath: src/features/settings/models/ModelStatus.tsx
// A model's availability as shown in every model row: the status badge with Download / Delete on
// the trailing side, and the progress bar, unsupported reason and failure message under the row.
import { Download, Trash2 } from "lucide-react";
import type { ModelInfo } from "../../../lib/api";
import { Badge, Button, IconButton, ProgressBar } from "../../../ui";
import type { ModelsState } from "./useModels";
import "./models.css";

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

interface SideProps {
    model: ModelInfo;
    state: ModelsState;
    /** Offer Delete for a downloaded local model. */
    deletable: boolean;
}

/** Status badge plus the Download (or Delete) action; sits in a row's `.sv-model__side`. */
export function ModelSide({ model, state, deletable }: SideProps) {
    const local = model.provider !== "groq";
    const fraction = state.progress[model.id];
    const downloading = fraction !== undefined || model.status === "downloading";
    return (
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
            {local && model.status === "ready" && deletable && (
                <IconButton
                    label={`Delete ${model.name}`}
                    icon={<Trash2 size={15} />}
                    onClick={() => {
                        void state.remove(model.id);
                    }}
                />
            )}
        </div>
    );
}

/** Download progress, why the model is unsupported, and the last failure; full row width. */
export function ModelDetails({ model, state }: { model: ModelInfo; state: ModelsState }) {
    const fraction = state.progress[model.id];
    const failure = state.failures[model.id];
    return (
        <>
            {(fraction !== undefined || model.status === "downloading") && (
                <div className="sv-model__progress">
                    <ProgressBar value={fraction ?? model.progress ?? 0} />
                    {state.pendingUse === model.id && (
                        <span className="sv-model__note">Switches to this model when ready</span>
                    )}
                </div>
            )}
            {model.status === "unsupported" && model.reason && (
                <p className="sv-model__reason">{model.reason}</p>
            )}
            {failure && (
                <p className="sv-model__error" role="alert">
                    {failure}
                </p>
            )}
        </>
    );
}
