// FilePath: src/features/insights/UsageCard.tsx
import type { AppUsage, CategoryUsage } from "../../lib/api";
import { CATEGORY_META } from "../../lib/categories";
import { formatCompact, formatNumber, pluralize } from "../../lib/format";
import { Card } from "../../ui";

// Largest share gets the deepest teal, then lighter tints down the list.
const BAR_TONES = ["var(--heat-4)", "var(--heat-3)", "var(--heat-2)", "var(--heat-1)"];

export interface UsageCardProps {
    categories: CategoryUsage[];
    topApps: AppUsage[];
}

export function UsageCard({ categories, topApps }: UsageCardProps) {
    const rows = categories
        .filter((category) => category.dictations > 0)
        .sort((a, b) => b.percent - a.percent);
    const apps = topApps.filter((app) => app.words > 0).slice(0, 5);
    const topWords = apps[0]?.words ?? 0;

    return (
        <Card className="insights-card usage-card">
            <h2 className="insights-card__title">Usage by app</h2>
            <p className="insights-card__caption">What kind of apps you dictate into</p>

            <ul className="usage-list">
                {rows.map((row, index) => {
                    const meta = CATEGORY_META[row.category];
                    const percent = Math.round(row.percent);
                    return (
                        <li key={row.category} className="usage-row">
                            <div className="usage-row__head">
                                <span className="usage-row__icon" aria-hidden="true">
                                    {meta.icon}
                                </span>
                                <span className="usage-row__label">{meta.label}</span>
                                <span className="usage-row__pill">{percent}%</span>
                                <span className="usage-row__count">
                                    {formatNumber(row.dictations)}{" "}
                                    {pluralize(row.dictations, "dictation")} ·{" "}
                                    {formatCompact(row.words)} {pluralize(row.words, "word")}
                                </span>
                            </div>
                            <svg
                                className="usage-row__bar"
                                viewBox="0 0 100 6"
                                preserveAspectRatio="none"
                                role="img"
                                aria-label={`${meta.label}: ${String(percent)}% of dictations`}
                            >
                                <rect className="usage-row__track" width={100} height={6} />
                                <rect
                                    width={Math.max(2, row.percent)}
                                    height={6}
                                    fill={BAR_TONES[Math.min(index, BAR_TONES.length - 1)]}
                                />
                            </svg>
                        </li>
                    );
                })}
            </ul>

            {apps.length > 0 && (
                <div className="top-apps">
                    <h3 className="caps-label">Top apps</h3>
                    <ol className="top-apps__list">
                        {apps.map((app) => (
                            <li key={app.bundleId ?? app.name} className="top-apps__item">
                                <span className="top-apps__name">{app.name}</span>
                                <span className="top-apps__bar" aria-hidden="true">
                                    <svg viewBox="0 0 100 4" preserveAspectRatio="none">
                                        <rect
                                            width={topWords > 0 ? (app.words / topWords) * 100 : 0}
                                            height={4}
                                        />
                                    </svg>
                                </span>
                                <span className="top-apps__words">
                                    {formatCompact(app.words)} {pluralize(app.words, "word")}
                                </span>
                            </li>
                        ))}
                    </ol>
                </div>
            )}
        </Card>
    );
}
