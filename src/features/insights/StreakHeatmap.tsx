// FilePath: src/features/insights/StreakHeatmap.tsx
import { useMemo, useState } from "react";
import type { DayActivity } from "../../lib/api";
import {
    formatLongDate,
    formatMonth,
    formatNumber,
    parseDayKey,
    pluralize,
} from "../../lib/format";
import "./heatmap.css";

interface Cell {
    day: DayActivity;
    date: Date;
    level: 0 | 1 | 2 | 3 | 4;
    inStreak: boolean;
}

interface Column {
    cells: (Cell | null)[];
    monthLabel: string | null;
}

const WEEKDAY_LABELS = ["", "Mon", "", "Wed", "", "Fri", ""];

function levelFor(words: number, max: number): Cell["level"] {
    if (words <= 0 || max <= 0) return 0;
    const share = words / max;
    if (share <= 0.25) return 1;
    if (share <= 0.5) return 2;
    if (share <= 0.75) return 3;
    return 4;
}

/**
 * Lays the days (oldest first) into week columns, Sunday on top, and marks the trailing run of
 * active days that makes up the current streak. Today may still be empty without breaking it.
 */
function buildColumns(days: DayActivity[], currentStreak: number): Column[] {
    const max = days.reduce((top, day) => Math.max(top, day.words), 0);
    const streakIndexes = new Set<number>();
    let index = days.length - 1;
    if (index >= 0 && days[index]?.dictations === 0) index -= 1;
    while (index >= 0 && streakIndexes.size < currentStreak && (days[index]?.dictations ?? 0) > 0) {
        streakIndexes.add(index);
        index -= 1;
    }

    const first = days[0];
    if (!first) return [];
    const leading = parseDayKey(first.date).getDay();
    const slots: (Cell | null)[] = Array.from({ length: leading }, () => null);
    days.forEach((day, position) => {
        slots.push({
            day,
            date: parseDayKey(day.date),
            level: levelFor(day.words, max),
            inStreak: streakIndexes.has(position),
        });
    });

    const columns: Column[] = [];
    let previousMonth = -1;
    for (let start = 0; start < slots.length; start += 7) {
        const cells = slots.slice(start, start + 7);
        while (cells.length < 7) cells.push(null);
        const firstReal = cells.find((cell): cell is Cell => cell !== null);
        let monthLabel: string | null = null;
        if (firstReal && firstReal.date.getMonth() !== previousMonth) {
            // Skip a label squeezed into the very first partial column; it would collide.
            if (previousMonth !== -1 || firstReal.date.getDate() <= 7) {
                monthLabel = formatMonth(firstReal.date);
            }
            previousMonth = firstReal.date.getMonth();
        }
        columns.push({ cells, monthLabel });
    }
    return columns;
}

export interface StreakHeatmapProps {
    days: DayActivity[];
    currentStreak: number;
}

export function StreakHeatmap({ days, currentStreak }: StreakHeatmapProps) {
    const columns = useMemo(() => buildColumns(days, currentStreak), [days, currentStreak]);
    const [hovered, setHovered] = useState<string | null>(null);
    const activeDays = days.filter((day) => day.dictations > 0).length;
    const summary =
        `Daily activity for the last 26 weeks: ` +
        `${String(activeDays)} active ${pluralize(activeDays, "day")}`;

    return (
        <div className="heatmap">
            <div
                className="heatmap__grid"
                role="img"
                aria-label={summary}
                onMouseLeave={() => {
                    setHovered(null);
                }}
            >
                <div className="heatmap__weekdays" aria-hidden="true">
                    <span className="heatmap__month" />
                    {WEEKDAY_LABELS.map((label, row) => (
                        <span key={row} className="heatmap__weekday">
                            {label}
                        </span>
                    ))}
                </div>
                {columns.map((column, columnIndex) => (
                    <div key={columnIndex} className="heatmap__column" aria-hidden="true">
                        <span className="heatmap__month">{column.monthLabel ?? ""}</span>
                        {column.cells.map((cell, row) => {
                            if (!cell) {
                                return (
                                    <span
                                        key={row}
                                        className="heatmap__cell heatmap__cell--blank"
                                    />
                                );
                            }
                            const classes = [
                                "heatmap__cell",
                                `heatmap__cell--${String(cell.level)}`,
                                cell.inStreak && "is-streak",
                            ]
                                .filter(Boolean)
                                .join(" ");
                            // Tooltips near either edge anchor to that side to stay in the card.
                            const edge =
                                columnIndex > columns.length - 7
                                    ? " is-end"
                                    : columnIndex < 6
                                      ? " is-start"
                                      : "";
                            return (
                                <span
                                    key={row}
                                    className={classes}
                                    onMouseEnter={() => {
                                        setHovered(cell.day.date);
                                    }}
                                >
                                    {hovered === cell.day.date && (
                                        <span className={`heatmap__tooltip${edge}`}>
                                            <strong>
                                                {cell.day.words > 0
                                                    ? `${formatNumber(cell.day.words)} ` +
                                                      pluralize(cell.day.words, "word")
                                                    : "No dictation"}
                                            </strong>
                                            <span>{formatLongDate(cell.date)}</span>
                                        </span>
                                    )}
                                </span>
                            );
                        })}
                    </div>
                ))}
            </div>
            <div className="heatmap__legend" aria-hidden="true">
                <span>Less</span>
                {[0, 1, 2, 3, 4].map((level) => (
                    <span key={level} className={`heatmap__cell heatmap__cell--${String(level)}`} />
                ))}
                <span>More</span>
                <span className="heatmap__legend-streak">
                    <span className="heatmap__cell heatmap__cell--3 is-streak" />
                    Current streak
                </span>
            </div>
        </div>
    );
}
