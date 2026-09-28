// FilePath: src/features/onboarding/ModelDownloadCard.tsx
// The voice model onboarding is about to use, when it runs on this Mac: what it offers and a
// one-click download with live progress. A new install lands here with Parakeet TDT v3 (the
// default), so the first thing a new user sees is the download that makes dictation work.
import { Check, Download } from "lucide-react";
import type { ModelInfo } from "../../lib/api";
import { Badge, Button, ProgressBar } from "../../ui";
import { formatSize, ProviderMark } from "../settings/models/VoiceModelList";
import type { ModelsState } from "../settings/models/useModels";
import "./ModelDownloadCard.css";

/** Mirrors `DEFAULT_TRANSCRIPTION_MODEL` in `crates/sv-domain/src/settings.rs`. */
export const RECOMMENDED_MODEL = "parakeet-tdt-v3";

interface Props {
    model: ModelInfo;
    state: ModelsState;
}

function DownloadState({ model, state }: Props) {
    const fraction = state.progress[model.id];
    const failure = state.failures[model.id];
    const size = formatSize(model.sizeMb);

    if (model.status === "ready") {
        return (
            <p className="sv-onb__granted" role="status">
                <Check size={16} strokeWidth={2.2} />
                Downloaded. Dictation runs on this Mac.
            </p>
        );
    }
    if (model.status === "unsupported") {
        return <p className="sv-onb__dl-note">{model.reason ?? "Not available on this Mac."}</p>;
    }
    if (fraction !== undefined || model.status === "downloading") {
        const value = fraction ?? model.progress ?? 0;
        return (
            <div className="sv-onb__dl-progress" role="status">
                <ProgressBar value={value} />
                <div className="sv-onb__dl-line">
                    <span className="sv-onb__dl-percent">
                        Downloading {Math.round(value * 100)}%
                    </span>
                    {size && <span className="sv-onb__dl-note">{size}</span>}
                </div>
            </div>
        );
    }
    return (
        <div className="sv-onb__dl-line">
            <div className="sv-onb__dl-text">
                {failure ? (
                    <span className="sv-model__error sv-onb__dl-error" role="alert">
                        {failure}
                    </span>
                ) : (
                    <span className="sv-onb__dl-note">
                        One-time download{size ? ` · ${size}` : ""}
                    </span>
                )}
            </div>
            <Button
                variant="primary"
                icon={<Download size={15} />}
                onClick={() => {
                    void state.download(model.id);
                }}
            >
                {failure ? "Try again" : "Download"}
            </Button>
        </div>
    );
}

export function ModelDownloadCard({ model, state }: Props) {
    return (
        <section className="sv-onb__panel sv-onb__dl" aria-label={`${model.name} download`}>
            <div className="sv-onb__dl-head">
                <ProviderMark provider={model.provider} />
                <div className="sv-model__text">
                    <span className="sv-onb__dl-name">
                        <span className="sv-model__name">{model.name}</span>
                        {model.id === RECOMMENDED_MODEL && <Badge tone="accent">Recommended</Badge>}
                    </span>
                    <span className="sv-model__subtitle">{model.subtitle}</span>
                </div>
            </div>
            <ul className="sv-onb__dl-points">
                <li>
                    <Check size={14} strokeWidth={2.2} />
                    Your voice never leaves this Mac
                </li>
                <li>
                    <Check size={14} strokeWidth={2.2} />
                    Works offline, no account or API key
                </li>
                <li>
                    <Check size={14} strokeWidth={2.2} />
                    {model.languages}
                </li>
            </ul>
            <div className="sv-onb__panel-foot">
                <DownloadState model={model} state={state} />
            </div>
        </section>
    );
}
