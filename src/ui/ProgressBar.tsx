// FilePath: src/ui/ProgressBar.tsx
import "./ProgressBar.css";

export interface ProgressBarProps {
    /** 0..1 */
    value: number;
    tone?: "accent" | "neutral";
}

export function ProgressBar({ value, tone = "accent" }: ProgressBarProps) {
    const clamped = Number.isFinite(value) ? Math.min(1, Math.max(0, value)) : 0;
    return (
        <div
            className={`sv-progress sv-progress--${tone}`}
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(clamped * 100)}
        >
            <div
                className="sv-progress__fill"
                style={{ transform: `scaleX(${String(clamped)})` }}
            />
        </div>
    );
}
