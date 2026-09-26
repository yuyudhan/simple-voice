// FilePath: src/features/insights/Meter.tsx

export interface MeterProps {
    percent: number;
    /** Only the leading item carries the signal colour. */
    lead: boolean;
    /** Makes the meter an image with this name; without it the meter is decorative. */
    label?: string;
}

/** A rounded horizontal meter. */
export function Meter({ percent, lead, label }: MeterProps) {
    const width = `${String(Math.min(100, Math.max(1.5, percent)))}%`;
    return (
        <span
            className="usage-meter"
            role={label ? "img" : undefined}
            aria-label={label}
            aria-hidden={label ? undefined : true}
        >
            <span className={`usage-meter__fill${lead ? " is-lead" : ""}`} style={{ width }} />
        </span>
    );
}
