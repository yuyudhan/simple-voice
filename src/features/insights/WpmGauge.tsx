// FilePath: src/features/insights/WpmGauge.tsx

const MAX_WPM = 220;
const TICK_STEP = 10;
const MAJOR_EVERY = 11;
// Dial centred at (100, 100): the scale is a semicircle of radius 76 drawn left to right.
const CX = 100;
const CY = 100;
const ARC_R = 76;
const ARC = "M 24 100 A 76 76 0 0 1 176 100";
const TICKS = Array.from({ length: MAX_WPM / TICK_STEP + 1 }, (_, index) => index);

function point(share: number, radius: number): { x: number; y: number } {
    const angle = Math.PI * (1 - share);
    return { x: CX + radius * Math.cos(angle), y: CY - radius * Math.sin(angle) };
}

/** Speaking speed as an instrument dial: a tick scale, a signal arc and a mono readout. */
export function WpmGauge({ wpm }: { wpm: number }) {
    const share = Math.min(1, Math.max(0, wpm / MAX_WPM));
    const pointerInner = point(share, ARC_R - 9);
    const pointerOuter = point(share, ARC_R + 9);
    const label = `${String(Math.round(wpm))} words per minute, on a scale up to 220`;
    return (
        <svg className="wpm-gauge" viewBox="0 0 200 118" role="img" aria-label={label}>
            {TICKS.map((tick) => {
                const major = tick % MAJOR_EVERY === 0;
                const tickShare = (tick * TICK_STEP) / MAX_WPM;
                const inner = point(tickShare, major ? ARC_R + 8 : ARC_R + 10);
                const outer = point(tickShare, ARC_R + 16);
                const lit = share > 0 && tickShare <= share;
                const classes = ["wpm-gauge__tick", lit && "is-lit", major && "is-major"]
                    .filter(Boolean)
                    .join(" ");
                return (
                    <line
                        key={tick}
                        className={classes}
                        x1={inner.x}
                        y1={inner.y}
                        x2={outer.x}
                        y2={outer.y}
                    />
                );
            })}
            <path d={ARC} className="wpm-gauge__track" pathLength={100} />
            {share > 0 && (
                <path
                    d={ARC}
                    className="wpm-gauge__value"
                    pathLength={100}
                    strokeDasharray={`${String(share * 100)} 100`}
                />
            )}
            <line
                className="wpm-gauge__pointer"
                x1={pointerInner.x}
                y1={pointerInner.y}
                x2={pointerOuter.x}
                y2={pointerOuter.y}
            />
            <text className="wpm-gauge__readout" x={CX} y={88} textAnchor="middle">
                {Math.round(wpm)}
            </text>
            <text className="wpm-gauge__unit" x={CX} y={104} textAnchor="middle">
                WPM
            </text>
            <text className="wpm-gauge__scale" x={CX - ARC_R} y={115} textAnchor="middle">
                0
            </text>
            <text className="wpm-gauge__scale" x={CX + ARC_R} y={115} textAnchor="middle">
                {MAX_WPM}
            </text>
        </svg>
    );
}
