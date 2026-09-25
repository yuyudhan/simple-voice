// FilePath: src/features/history/StatsStrip.tsx
import { useCallback, useEffect, useState } from "react";
import { api, errorMessage, events, type Insights } from "../../lib/api";
import { dayKey, formatCompact, parseDayKey, pluralize, startOfWeek } from "../../lib/format";
import { useTauriEvent } from "../../lib/useTauriEvent";
import { useToast } from "../../ui";

interface Stats {
    today: number;
    week: number;
    streak: number;
}

function summarize(insights: Insights): Stats {
    const now = new Date();
    const todayKey = dayKey(now);
    const weekStart = startOfWeek(now).getTime();
    let today = 0;
    let week = 0;
    for (const day of insights.days) {
        if (day.date === todayKey) today = day.words;
        if (parseDayKey(day.date).getTime() >= weekStart) week += day.words;
    }
    return { today, week, streak: insights.currentStreak };
}

export function StatsStrip() {
    const { toast } = useToast();
    const [stats, setStats] = useState<Stats | null>(null);

    const load = useCallback(
        () =>
            api.getInsights().then(
                (insights) => {
                    setStats(summarize(insights));
                },
                (error: unknown) => {
                    toast(errorMessage(error), "danger");
                },
            ),
        [toast],
    );

    useEffect(() => {
        void load();
    }, [load]);

    useTauriEvent(events.historyChanged, () => {
        void load();
    });

    const items = [
        { label: "Words today", value: stats ? formatCompact(stats.today) : "–" },
        { label: "This week", value: stats ? formatCompact(stats.week) : "–" },
        {
            label: "Streak",
            value: stats ? `${String(stats.streak)} ${pluralize(stats.streak, "day")}` : "–",
        },
    ];

    return (
        <dl className="history-stats">
            {items.map((item) => (
                <div key={item.label} className="history-stats__item">
                    <dt className="caps-label">{item.label}</dt>
                    <dd className="history-stats__value">{item.value}</dd>
                </div>
            ))}
        </dl>
    );
}
