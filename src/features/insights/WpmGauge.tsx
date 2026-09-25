// FilePath: src/features/insights/WpmGauge.tsx
import { useId } from "react";

const MAX_WPM = 220;
// Semicircle centred at (100, 100) with radius 80, drawn left to right over the top.
const ARC = "M 20 100 A 80 80 0 0 1 180 100";

export function WpmGauge({ wpm }: { wpm: number }) {
    // useId output contains characters that break a `url(#id)` paint reference.
    const gradientId = `wpm-${useId().replace(/[^\w-]/g, "")}`;
    const share = Math.min(1, Math.max(0, wpm / MAX_WPM));
    const angle = Math.PI * (1 - share);
    const knobX = 100 + 80 * Math.cos(angle);
    const knobY = 100 - 80 * Math.sin(angle);
    const label = `${String(Math.round(wpm))} words per minute, on a scale up to 220`;
    return (
        <svg className="wpm-gauge" viewBox="0 0 200 118" role="img" aria-label={label}>
            <defs>
                <linearGradient id={gradientId} x1="0" x2="1" y1="0" y2="0">
                    <stop offset="0%" stopColor="var(--heat-2)" />
                    <stop offset="100%" stopColor="var(--heat-4)" />
                </linearGradient>
            </defs>
            <path d={ARC} className="wpm-gauge__track" pathLength={100} />
            {share > 0 && (
                <path
                    d={ARC}
                    className="wpm-gauge__value"
                    pathLength={100}
                    stroke={`url(#${gradientId})`}
                    strokeDasharray={`${String(share * 100)} 100`}
                />
            )}
            {share > 0 && <circle className="wpm-gauge__knob" cx={knobX} cy={knobY} r={6} />}
            <text className="wpm-gauge__value-text" x={100} y={88} textAnchor="middle">
                {Math.round(wpm)}
            </text>
            <text className="wpm-gauge__unit" x={100} y={106} textAnchor="middle">
                words / min
            </text>
            <text className="wpm-gauge__scale" x={20} y={116} textAnchor="middle">
                0
            </text>
            <text className="wpm-gauge__scale" x={180} y={116} textAnchor="middle">
                {MAX_WPM}
            </text>
        </svg>
    );
}
