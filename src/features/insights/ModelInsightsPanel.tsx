// FilePath: src/features/insights/ModelInsightsPanel.tsx
import type { ReactNode } from "react";
import { AudioLines, Gauge, Sparkles } from "lucide-react";
import { api, type ModelTiming } from "../../lib/api";
import { formatNumber, formatSeconds, pluralize } from "../../lib/format";
import { Card, EmptyState } from "../../ui";
import { Meter } from "./Meter";
import { useInsightsData, InsightsPending } from "./useInsightsData";

const EMPTY_DESCRIPTION =
    "Every dictation from now on records how long transcription and formatting took, so you " +
    "can compare the models you use.";

interface StageCardProps {
    title: string;
    caption: string;
    icon: ReactNode;
    models: ModelTiming[];
    empty: string;
    /** The stage's throughput figure, e.g. "12× real time". */
    speed: (model: ModelTiming) => string | null;
}

function realTime(model: ModelTiming): string | null {
    if (model.totalMs <= 0 || model.audioMs <= 0) return null;
    return `${(model.audioMs / model.totalMs).toFixed(1)}× real time`;
}

function wordsPerSecond(model: ModelTiming): string | null {
    if (model.totalMs <= 0 || model.words <= 0) return null;
    return `${formatNumber(Math.round((model.words * 1000) / model.totalMs))} words/s`;
}

function StageCard({ title, caption, icon, models, empty, speed }: StageCardProps) {
    const slowestAverage = Math.max(0, ...models.map((model) => model.averageMs));
    const fastestAverage = Math.min(...models.map((model) => model.averageMs));
    return (
        <Card className="insights-card">
            <div>
                <h2 className="insights-card__title">{title}</h2>
                <p className="insights-card__caption">{caption}</p>
            </div>
            {models.length === 0 ? (
                <p className="insights-card__foot">{empty}</p>
            ) : (
                <ul className="usage-list">
                    {models.map((model) => {
                        const rate = speed(model);
                        const average = formatSeconds(model.averageMs);
                        return (
                            <li key={model.model} className="usage-row">
                                <div className="usage-row__head">
                                    <span className="usage-row__icon" aria-hidden="true">
                                        {icon}
                                    </span>
                                    <span className="usage-row__label" title={model.model}>
                                        {model.name}
                                    </span>
                                    <span className="usage-row__count">
                                        {formatNumber(model.runs)} {pluralize(model.runs, "run")}
                                    </span>
                                    <span className="usage-row__percent readout">{average}</span>
                                </div>
                                <Meter
                                    percent={
                                        slowestAverage > 0
                                            ? (model.averageMs / slowestAverage) * 100
                                            : 0
                                    }
                                    lead={models.length > 1 && model.averageMs === fastestAverage}
                                    label={`${model.name}: ${average} on average`}
                                />
                                <p className="model-speed__detail">
                                    Fastest {formatSeconds(model.fastestMs)} · slowest{" "}
                                    {formatSeconds(model.slowestMs)}
                                    {rate && ` · ${rate}`}
                                </p>
                            </li>
                        );
                    })}
                </ul>
            )}
        </Card>
    );
}

/** How fast each transcription and formatting model has been on this Mac. */
export function ModelInsightsPanel() {
    const { data, failed } = useInsightsData(api.getModelInsights);

    if (!data) return <InsightsPending failed={failed} />;

    if (data.transcription.length === 0 && data.formatting.length === 0) {
        return (
            <EmptyState
                icon={<Gauge />}
                title="Model speed will appear here"
                description={EMPTY_DESCRIPTION}
            />
        );
    }

    return (
        <div className="insights-row insights-row--pair">
            <StageCard
                title="Transcription"
                caption="Average time from audio to text"
                icon={<AudioLines />}
                models={data.transcription}
                empty="No timed transcriptions yet."
                speed={realTime}
            />
            <StageCard
                title="Formatting"
                caption="Average time for the AI formatting pass"
                icon={<Sparkles />}
                models={data.formatting}
                empty="No formatted dictations yet. Turn on AI formatting in Settings → Models."
                speed={wordsPerSecond}
            />
        </div>
    );
}
