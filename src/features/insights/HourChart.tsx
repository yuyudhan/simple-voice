// FilePath: src/features/insights/HourChart.tsx
import { useState } from "react";
import type { HourActivity } from "../../lib/api";
import { formatNumber, pluralize } from "../../lib/format";
import { Card } from "../../ui";
import "./charts.css";

const hourFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric" });
const AXIS_HOURS = [0, 6, 12, 18];

function hourLabel(hour: number): string {
    return hourFormat.format(new Date(2000, 0, 1, hour));
}

function hourRange(hour: number): string {
    return `${hourLabel(hour)} – ${hourLabel((hour + 1) % 24)}`;
}

export interface HourChartProps {
    hours: HourActivity[];
}

/** Words dictated per local hour of the day; the peak hour alone carries the signal colour. */
export function HourChart({ hours }: HourChartProps) {
    const [hovered, setHovered] = useState<number | null>(null);
    // The earliest hour wins a tie; null when nothing was dictated.
    const peak = hours.reduce<HourActivity | null>(
        (best, hour) => (hour.words > (best?.words ?? 0) ? hour : best),
        null,
    );
    const max = peak?.words ?? 0;
    const summary = peak
        ? `Words dictated by hour of day. Most active: ${hourRange(peak.hour)}`
        : "Words dictated by hour of day";

    return (
        <Card className="insights-card hour-card">
            <div className="streak-card__head">
                <div>
                    <h2 className="insights-card__title">Time of day</h2>
                    <p className="insights-card__caption">When you dictate, in local time</p>
                </div>
                {peak && (
                    <dl className="streak-card__figures">
                        <div>
                            <dt className="caps-label">Most active</dt>
                            <dd className="streak-card__value">
                                <span className="readout insights-figure--small">
                                    {hourLabel(peak.hour)}
                                </span>
                            </dd>
                        </div>
                    </dl>
                )}
            </div>

            <div
                className="hour-chart"
                role="img"
                aria-label={summary}
                onMouseLeave={() => {
                    setHovered(null);
                }}
            >
                <div className="hour-chart__bars" aria-hidden="true">
                    {hours.map((hour) => {
                        const height = max > 0 ? (hour.words / max) * 100 : 0;
                        const classes = [
                            "hour-chart__bar",
                            hour.words === 0 && "is-empty",
                            hour.hour === peak?.hour && "is-peak",
                        ]
                            .filter(Boolean)
                            .join(" ");
                        const edge = hour.hour < 3 ? " is-start" : hour.hour > 20 ? " is-end" : "";
                        return (
                            <span
                                key={hour.hour}
                                className="hour-chart__slot"
                                onMouseEnter={() => {
                                    setHovered(hour.hour);
                                }}
                            >
                                <span
                                    className={classes}
                                    style={{ height: `${String(Math.max(height, 2))}%` }}
                                />
                                {hovered === hour.hour && (
                                    <span className={`chart-tooltip${edge}`}>
                                        <strong>
                                            {hour.words > 0
                                                ? `${formatNumber(hour.words)} ` +
                                                  `${pluralize(hour.words, "word")} · ` +
                                                  `${formatNumber(hour.dictations)} ` +
                                                  pluralize(hour.dictations, "dictation")
                                                : "No dictation"}
                                        </strong>
                                        <span>{hourRange(hour.hour)}</span>
                                    </span>
                                )}
                            </span>
                        );
                    })}
                </div>
                <div className="hour-chart__axis" aria-hidden="true">
                    {AXIS_HOURS.map((hour) => (
                        <span key={hour}>{hourLabel(hour)}</span>
                    ))}
                </div>
            </div>
        </Card>
    );
}
