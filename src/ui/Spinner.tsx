// FilePath: src/ui/Spinner.tsx
import "./Spinner.css";

export function Spinner({ size = 16 }: { size?: number }) {
    return (
        <span
            className="sv-spinner"
            role="status"
            aria-label="Loading"
            style={{ width: size, height: size, borderWidth: Math.max(1.5, size / 9) }}
        />
    );
}
