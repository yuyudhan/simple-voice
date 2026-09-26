// FilePath: src/features/insights/BestsCard.tsx
import type { PersonalBests } from "../../lib/api";
import { formatLongDate, formatNumber, parseDayKey, pluralize } from "../../lib/format";
import { Card } from "../../ui";

interface BestRecord {
    label: string;
    value: number;
    unit: string;
    date: string | null;
}

export interface BestsCardProps {
    bests: PersonalBests;
}

/** All-time records, each with the day it was set. */
export function BestsCard({ bests }: BestsCardProps) {
    const records: BestRecord[] = [
        {
            label: "Longest dictation",
            value: bests.longestDictationWords,
            unit: pluralize(bests.longestDictationWords, "word"),
            date: null,
        },
        {
            label: "Biggest day",
            value: bests.bestDayWords,
            unit: pluralize(bests.bestDayWords, "word"),
            date: bests.bestDayDate,
        },
        {
            label: "Busiest day",
            value: bests.busiestDayDictations,
            unit: pluralize(bests.busiestDayDictations, "dictation"),
            date: bests.busiestDayDate,
        },
    ];

    return (
        <Card className="insights-card">
            <div>
                <h2 className="insights-card__title">Personal bests</h2>
                <p className="insights-card__caption">Your records across all history</p>
            </div>
            <dl className="bests-list">
                {records.map((record) => (
                    <div key={record.label} className="fix-stat">
                        <dt className="caps-label">{record.label}</dt>
                        <dd className="bests-list__value">
                            <span className="readout insights-figure--small">
                                {formatNumber(record.value)}
                            </span>{" "}
                            {record.unit}
                            {record.date && (
                                <span className="bests-list__date">
                                    {formatLongDate(parseDayKey(record.date))}
                                </span>
                            )}
                        </dd>
                    </div>
                ))}
            </dl>
        </Card>
    );
}
