// FilePath: src/features/insights/InsightsPage.tsx
import { useCallback, useEffect, useState } from "react";
import { ChartColumn, Flame, Info, TrendingDown, TrendingUp } from "lucide-react";
import { api, errorMessage, events, type Insights } from "../../lib/api";
import { formatMinutes, formatNumber, pluralize } from "../../lib/format";
import { useTauriEvent } from "../../lib/useTauriEvent";
import { Card, EmptyState, Spinner, useToast } from "../../ui";
import { StreakHeatmap } from "./StreakHeatmap";
import { UsageCard } from "./UsageCard";
import { WpmGauge } from "./WpmGauge";
import "./insights.css";

const WORDS_PER_PAGE = 500;
// The backend's time-saved figure assumes this typing speed too.
const TYPING_WPM = 40;
const EMPTY_DESCRIPTION =
    "Dictate anything, anywhere. Your speed, streaks and favourite apps show up after the " +
    "first dictation.";
const TIP_DICTIONARY = "Times a replacement rule from your dictionary rewrote what was heard.";
const TIP_CORRECTED =
    "Words changed between the raw transcript and what was pasted: fillers dropped, " +
    "self-corrections applied, spellings fixed.";
const TIP_SAVED = `Compared with typing the same words at ${String(TYPING_WPM)} words per minute.`;

function InfoTip({ text }: { text: string }) {
    return (
        <span className="info-tip" tabIndex={0} role="note" aria-label={text} data-tip={text}>
            <Info aria-hidden="true" />
        </span>
    );
}

function MonthChange({ insights }: { insights: Insights }) {
    const change = insights.monthChangePercent;
    if (change === null) {
        return insights.wordsThisMonth > 0 ? (
            <span className="month-pill month-pill--up">New this month</span>
        ) : null;
    }
    const rounded = Math.round(change);
    const up = rounded >= 0;
    return (
        <span className={`month-pill ${up ? "month-pill--up" : "month-pill--down"}`}>
            {up ? <TrendingUp aria-hidden="true" /> : <TrendingDown aria-hidden="true" />}
            {up ? "+" : "−"}
            {Math.abs(rounded)}% this month
        </span>
    );
}

function equivalence(totalWords: number): string {
    const pages = totalWords / WORDS_PER_PAGE;
    if (pages < 1) {
        const percent = Math.max(1, Math.round(pages * 100));
        return `That's ${String(percent)}% of a page of writing.`;
    }
    const rounded = pages < 10 ? Math.round(pages * 10) / 10 : Math.round(pages);
    return `That's about ${formatNumber(rounded)} ${pluralize(rounded, "page")} of writing.`;
}

export function InsightsPage() {
    const { toast } = useToast();
    const [insights, setInsights] = useState<Insights | null>(null);
    const [failed, setFailed] = useState<string | null>(null);

    const load = useCallback(
        () =>
            api.getInsights().then(
                (next) => {
                    setInsights(next);
                    setFailed(null);
                },
                (error: unknown) => {
                    setFailed(errorMessage(error));
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

    const header = (
        <header className="sv-page-header">
            <div>
                <h1 className="page-title">Insights</h1>
                <p className="page-subtitle">How you use your voice.</p>
            </div>
        </header>
    );

    if (!insights) {
        return (
            <>
                {header}
                {failed ? (
                    <p className="insights-error" role="alert">
                        {failed}
                    </p>
                ) : (
                    <div className="insights-loading">
                        <Spinner size={20} />
                    </div>
                )}
            </>
        );
    }

    if (insights.totalDictations === 0) {
        return (
            <>
                {header}
                <EmptyState
                    icon={<ChartColumn />}
                    title="Your insights will appear here"
                    description={EMPTY_DESCRIPTION}
                />
            </>
        );
    }

    const speedup = insights.wordsPerMinute / TYPING_WPM;
    return (
        <>
            {header}
            <div className="insights-row insights-row--three">
                <Card className="insights-card">
                    <h2 className="insights-card__title">Speaking speed</h2>
                    <WpmGauge wpm={insights.wordsPerMinute} />
                    <p className="insights-card__foot">
                        {speedup >= 1.1
                            ? `About ${speedup.toFixed(1)}× faster than typing`
                            : "Speaking pace across all dictations"}
                    </p>
                </Card>

                <Card className="insights-card">
                    <h2 className="insights-card__title">Fixes made</h2>
                    <div className="fix-stat">
                        <span className="insights-big">
                            {formatNumber(insights.dictionaryFixes)}
                        </span>
                        <span className="caps-label">
                            Dictionary fixes <InfoTip text={TIP_DICTIONARY} />
                        </span>
                    </div>
                    <div className="fix-stat">
                        <span className="insights-big">
                            {formatNumber(insights.wordsCorrected)}
                        </span>
                        <span className="caps-label">
                            Words corrected <InfoTip text={TIP_CORRECTED} />
                        </span>
                    </div>
                </Card>

                <Card className="insights-card">
                    <div className="insights-card__title-row">
                        <h2 className="insights-card__title">Total words dictated</h2>
                        <MonthChange insights={insights} />
                    </div>
                    <span className="insights-hero-number">
                        {formatNumber(insights.totalWords)}
                    </span>
                    <p className="insights-card__foot">{equivalence(insights.totalWords)}</p>
                    <div className="saved">
                        <span className="caps-label">Time saved</span>
                        <span className="saved__value">
                            {formatMinutes(insights.timeSavedMinutes)}
                        </span>
                        <InfoTip text={TIP_SAVED} />
                    </div>
                    <p className="insights-card__foot">
                        {formatNumber(insights.totalDictations)}{" "}
                        {pluralize(insights.totalDictations, "dictation")}
                    </p>
                </Card>
            </div>

            <div className="insights-row insights-row--two">
                <UsageCard categories={insights.categories} topApps={insights.topApps} />

                <Card className="insights-card streak-card">
                    <div className="streak-card__head">
                        <Flame className="streak-card__flame" aria-hidden="true" />
                        <div>
                            <h2 className="streak-card__title">
                                {insights.currentStreak} day streak
                            </h2>
                            <p className="insights-card__caption">
                                Longest streak: {insights.longestStreak}{" "}
                                {pluralize(insights.longestStreak, "day")}
                            </p>
                        </div>
                    </div>
                    <StreakHeatmap days={insights.days} currentStreak={insights.currentStreak} />
                </Card>
            </div>
        </>
    );
}
